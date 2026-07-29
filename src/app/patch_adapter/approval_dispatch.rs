use super::*;

pub(super) struct ApprovalDispatch {
    pub(super) report: String,
    pub(super) verification_credential: Option<OneShotSecret>,
}

impl ApprovalDispatch {
    fn without_secret(report: String) -> Self {
        Self {
            report,
            verification_credential: None,
        }
    }

    fn write_cli(mut self, proposal_id: &str) -> Result<(), AppError> {
        use std::io::Write;

        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(self.report.as_bytes())
            .map_err(|err| AppError::runtime(format!("patch approve 출력 실패: {err}")))?;
        if let Some(credential) = self.verification_credential.take() {
            stdout
                .write_all(
                    format!(
                        "\n- verification command approval: rpotato patch verify {proposal_id} --token "
                    )
                    .as_bytes(),
                )
                .map_err(|err| AppError::runtime(format!("patch approve 출력 실패: {err}")))?;
            credential
                .expose(|plaintext| stdout.write_all(plaintext.as_bytes()))
                .map_err(|err| AppError::runtime(format!("patch credential 출력 실패: {err}")))?;
        }
        stdout
            .write_all(b"\n")
            .and_then(|_| stdout.flush())
            .map_err(|err| AppError::runtime(format!("patch approve 출력 실패: {err}")))
    }

    #[cfg(test)]
    pub(super) fn into_test_report(mut self, proposal_id: &str) -> String {
        if let Some(credential) = self.verification_credential.take() {
            credential.expose(|plaintext| {
                self.report
                    .push_str("\n- verification command approval: rpotato patch verify ");
                self.report.push_str(proposal_id);
                self.report.push_str(" --token ");
                self.report.push_str(plaintext);
            });
        }
        self.report
    }
}

pub fn approve_to_stdout(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<(), AppError> {
    let intent_id = format!("intent-approve-{proposal_id}");
    approve_dispatch_for_intent(
        proposal_id,
        token,
        dry_run,
        verify_command,
        &intent_id,
        None,
    )?
    .write_cli(proposal_id)
}

#[cfg(test)]
pub fn approve_report(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
) -> Result<String, AppError> {
    let intent_id = format!("intent-approve-{proposal_id}");
    approve_report_for_intent(proposal_id, token, dry_run, verify_command, &intent_id)
}

#[cfg(test)]
pub(crate) fn approve_report_for_intent(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
    intent_id: &str,
) -> Result<String, AppError> {
    approve_dispatch_for_intent(proposal_id, token, dry_run, verify_command, intent_id, None)
        .map(|dispatch| dispatch.into_test_report(proposal_id))
}

fn approve_dispatch_for_intent(
    proposal_id: &str,
    token: &str,
    dry_run: bool,
    verify_command: Option<&str>,
    intent_id: &str,
    expected_lease: Option<&SelectionLease>,
) -> Result<ApprovalDispatch, AppError> {
    validate_proposal_id(proposal_id)?;
    validate_outcome_id(intent_id, "intent")?;
    ensure_source_install_platform_supported(cfg!(unix), std::env::consts::OS, dry_run)?;
    let proposal_path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.txt"));
    let record = load_proposal_record(proposal_id, &proposal_path)?;
    if record.workflow_id.is_empty() {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: standalone preview는 diff-only이며 mutation 권위가 없습니다.\n- 동작: rpotato run으로 canonical workflow proposal을 생성하세요.",
        ));
    }

    if verify_command.is_some() {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: patch 적용 승인과 verification command 승인은 분리되어 있습니다.\n- 동작: patch approve 후 발급되는 credential로 rpotato patch verify를 실행하세요.",
        ));
    }

    if dry_run {
        let discovered_active = state::active_workflow_id()?;
        let workflow =
            load_validated_approval_workflow(&record, token, discovered_active.as_deref())?;
        if workflow.phase == "complete" {
            validate_completed_workflow(&workflow)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            return Ok(ApprovalDispatch::without_secret(success_report(&workflow)));
        }
        if workflow.phase == "failed" {
            return Err(AppError::blocked(failure_report(&workflow)));
        }
        return dry_run_approval_report(&record, verify_command)
            .map(ApprovalDispatch::without_secret);
    }

    approval_prelock_test_barrier()?;
    let _approval_lock = ApprovalLock::acquire(&record.proposal_id)?;
    let discovered_active = state::active_workflow_id()?;
    let workflow = load_validated_approval_workflow(&record, token, discovered_active.as_deref())?;
    if workflow.phase == "complete" {
        validate_completed_workflow(&workflow)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Ok(ApprovalDispatch::without_secret(success_report(&workflow)));
    }
    if workflow.phase == "failed" {
        return Err(AppError::blocked(failure_report(&workflow)));
    }
    if workflow.phase == "pending-verification-approval"
        && prepared_approval_receipt_exists(&record, &workflow, intent_id)?
    {
        return Ok(ApprovalDispatch::without_secret(format!(
            "patch approve\n- status: refresh-only\n- code: secret.refresh-only\n- proposal id: {}\n- workflow id: {}\n- intent: {}\n- applied sha256: {}\n- verification approval: pending\n- boundary: 동일 intent의 exact E0..E9 커밋 영수증만 반환하며 approval token 또는 verification credential을 다시 표시하지 않습니다.",
            record.proposal_id,
            workflow.workflow_id,
            intent_id,
            record.proposed_sha256,
        )));
    }

    if workflow.phase == "pending-approval" {
        if workflow.active_skill_id.is_empty() {
            return Err(AppError::blocked(
                "patch approve 차단\n- 이유: active built-in skill이 없는 legacy workflow는 exact prepared E0..E9 트랜잭션을 사용할 수 없습니다.\n- 동작: 새 canonical workflow proposal을 생성하세요.",
            ));
        }
        return approve_prepared_skill_transaction(record, workflow, intent_id, expected_lease);
    }

    Err(AppError::blocked(format!(
        "patch approve 차단\n- 이유: workflow phase가 exact prepared approval을 허용하지 않습니다.\n- phase: {}",
        workflow.phase
    )))
}

pub(super) fn ensure_source_install_platform_supported(
    is_unix: bool,
    platform: &str,
    dry_run: bool,
) -> Result<(), AppError> {
    if !is_unix && !dry_run {
        return Err(AppError::blocked(
            unsupported_source_platform_outcome(platform)?.safe_message,
        ));
    }
    Ok(())
}

pub(crate) fn approve_for_tui(
    proposal_id: &str,
    token: &str,
    intent_id: &str,
    lease: &SelectionLease,
) -> Result<Option<OneShotSecret>, AppError> {
    let dispatch =
        approve_dispatch_for_intent(proposal_id, token, false, None, intent_id, Some(lease))?;
    Ok(dispatch.verification_credential)
}
