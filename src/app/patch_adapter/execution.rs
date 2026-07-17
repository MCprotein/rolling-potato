use super::*;

pub(super) fn apply_proposal(record: &ProposalRecord) -> Result<ApplyResult, AppError> {
    let target = resolve_target_for("patch approve", &record.relative_path)?;
    let read_decision = policy::classify_path(PathMode::Read, &target.relative_path)?;
    if read_decision.decision != Decision::Allow {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: target read policy가 allow가 아닙니다.\n- path: {}\n- decision: {}",
            target.relative_path,
            read_decision_label(read_decision.decision)
        )));
    }
    let write_decision = policy::classify_path(PathMode::Write, &target.relative_path)?;
    if write_decision.decision == Decision::Deny {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: target write policy가 deny입니다.\n- path: {}",
            target.relative_path
        )));
    }
    let metadata = fs::metadata(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch approve 대상 파일 metadata를 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::usage(format!(
            "patch approve 대상은 file이어야 합니다: {}",
            target.relative_path
        )));
    }
    if metadata.len() > MAX_PATCH_FILE_BYTES {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: 대상 파일이 patch 한도를 초과했습니다.\n- path: {}\n- size bytes: {}\n- max bytes: {}",
            target.relative_path,
            metadata.len(),
            MAX_PATCH_FILE_BYTES
        )));
    }

    let mut current = fs::read_to_string(&target.absolute_path).map_err(|err| {
        AppError::runtime(format!(
            "patch approve 대상 파일을 UTF-8 text로 읽지 못했습니다: {} ({err})",
            target.relative_path
        ))
    })?;
    let source_intent_id = format!("intent-source-{}", record.proposal_id);
    let identity = ledger::validated_current_identity()?;
    let pending_journal =
        paths::project_transition_journal_file(&identity.project_id, &source_intent_id);
    if pending_journal.exists() {
        let body = fs::read_to_string(&pending_journal).map_err(|err| {
            AppError::blocked(format!("prepared source journal 읽기 실패: {err}"))
        })?;
        let bundle = transition::parse_prepared_source_bundle(&body)?;
        let source_install = bundle
            .source_install
            .as_ref()
            .ok_or_else(|| AppError::blocked("prepared source journal source_install_v1 누락"))?;
        if bundle.intent_id != source_intent_id
            || source_install.before_sha256 != record.original_sha256
            || source_install.proposed_sha256 != record.proposed_sha256
        {
            return Err(AppError::blocked(
                "prepared source journal proposal binding 불일치",
            ));
        }
        state::install_prepared_source_bundle(&bundle, &pending_journal)?;
        transition::remove_committed_source_bundle(&bundle, &pending_journal)?;
        current = fs::read_to_string(&target.absolute_path).map_err(|err| {
            AppError::blocked(format!("recovered source target 읽기 실패: {err}"))
        })?;
    }
    let current_sha256 = sha256_text(&current);
    let rollback_path = transition::source_install_rollback_path(
        &source_intent_id,
        &record.proposal_id,
        &target.absolute_path,
        &record.original_sha256,
        &record.proposed_sha256,
    )?;
    let rollback_sha256 = if current_sha256 == record.proposed_sha256 && rollback_path.is_file() {
        Some(sha256_bytes(&fs::read(&rollback_path).map_err(|err| {
            AppError::blocked(format!(
                "patch approve 차단\n- 이유: rollback record를 읽지 못했습니다.\n- error: {err}"
            ))
        })?))
    } else {
        None
    };
    if application_domain::admit_apply(
        &target.relative_path,
        &current_sha256,
        &record.original_sha256,
        &record.proposed_sha256,
        rollback_sha256.as_deref(),
    )? == ApplyAdmission::AlreadyApplied
    {
        return Ok(ApplyResult {
            relative_path: target.relative_path,
            original_sha256: record.original_sha256.clone(),
            applied_sha256: record.proposed_sha256.clone(),
            rollback_path,
        });
    }

    let source_plan = transition::prepare_source_install_v1(
        &source_intent_id,
        &record.proposal_id,
        &target.absolute_path,
        current.as_bytes(),
        record.proposed_content.as_bytes(),
    )?;
    let bundle = transition::prepare_source_bundle(
        &source_intent_id,
        (!record.workflow_id.is_empty()).then_some(record.workflow_id.as_str()),
        source_plan,
        current.as_bytes(),
        record.proposed_content.as_bytes(),
    )?;
    let journal_path = transition::commit_prepared_source_bundle(&bundle)?;
    if let Err(err) = state::install_prepared_source_bundle(&bundle, &journal_path) {
        return Err(AppError::blocked(format!(
            "patch approve 복구 필요\n- code: source-install.recovery-required\n- path: {}\n- error: {}\n- journal: {}\n- 동작: committed journal과 rollback/guard 증거를 보존했습니다.",
            target.relative_path,
            err.message,
            journal_path.display()
        )));
    }
    transition::remove_committed_source_bundle(&bundle, &journal_path)?;

    let applied = fs::read_to_string(&target.absolute_path).map_err(|err| {
        let rollback = restore_bytes(
            &target.absolute_path,
            current.as_bytes(),
            &record.proposed_sha256,
            &record.original_sha256,
        );
        AppError::blocked(format!(
            "patch approve 실패\n- 이유: 적용 후 대상 파일을 읽지 못했습니다.\n- path: {}\n- error: {}\n- rollback status: {}",
            target.relative_path, err, rollback.status
        ))
    })?;
    let applied_sha256 = sha256_text(&applied);
    if applied_sha256 != record.proposed_sha256 {
        let rollback = restore_bytes(
            &target.absolute_path,
            current.as_bytes(),
            &record.proposed_sha256,
            &record.original_sha256,
        );
        return Err(AppError::blocked(format!(
            "patch approve 실패\n- 이유: 적용 후 SHA-256이 proposal과 일치하지 않습니다.\n- path: {}\n- expected proposed sha256: {}\n- applied sha256: {}\n- rollback status: {}",
            target.relative_path, record.proposed_sha256, applied_sha256, rollback.status
        )));
    }

    Ok(ApplyResult {
        relative_path: target.relative_path,
        original_sha256: record.original_sha256.clone(),
        applied_sha256,
        rollback_path,
    })
}

pub(super) fn build_verification_plan(command: &str) -> Result<VerificationPlan, AppError> {
    verification_domain::build_plan(command)
}

pub(super) fn run_verification(plan: &VerificationPlan) -> VerificationResult {
    let project_root =
        fs::canonicalize(paths::project_root()).unwrap_or_else(|_| paths::project_root());
    let output = ProcessCommand::new(&plan.argv[0])
        .args(&plan.argv[1..])
        .current_dir(project_root)
        .output();

    match output {
        Ok(output) => VerificationResult::from_output(
            plan,
            output.status.code(),
            &output.stdout,
            &output.stderr,
        ),
        Err(err) => VerificationResult::spawn_error(plan, &err.to_string()),
    }
}

pub(super) fn format_verification_result(result: &VerificationResult) -> String {
    format!(
        "- verification command: {}\n- verification exit code: {}\n- verification stdout: {}\n- verification stderr: {}\n",
        ledger::redact_text(&result.command),
        result.exit_code,
        result.stdout,
        result.stderr
    )
}

pub(super) fn restore_from_rollback(
    record: &ProposalRecord,
    rollback_path: &Path,
) -> RollbackResult {
    let target = match resolve_target_for("patch rollback", &record.relative_path) {
        Ok(target) => target,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!("restore-failed: {}", err.message),
            }
        }
    };
    let current = match fs::read(&target.absolute_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!("restore-conflict: target reread failed: {err}"),
            }
        }
    };
    let current_hash = sha256_bytes(&current);
    match application_domain::admit_rollback(
        &current_hash,
        &record.original_sha256,
        &record.proposed_sha256,
    ) {
        RollbackAdmission::AlreadyRestored(result) | RollbackAdmission::Conflict(result) => {
            return result;
        }
        RollbackAdmission::Ready => {}
    }
    let original = match fs::read(rollback_path) {
        Ok(contents) => contents,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!("restore-failed: rollback record read error: {err}"),
            }
        }
    };
    if let Err(result) = application_domain::validate_rollback_record(
        &sha256_bytes(&original),
        &record.original_sha256,
    ) {
        return result;
    }
    restore_bytes(
        &target.absolute_path,
        &original,
        &record.proposed_sha256,
        &record.original_sha256,
    )
}
