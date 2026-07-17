use super::*;

pub(super) fn validate_outcome_id(value: &str, kind: &str) -> Result<(), AppError> {
    let valid = !value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        });
    if valid {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "결과 식별자 검증 차단\n- kind: {kind}\n- 동작: 신뢰할 수 없는 식별자를 출력하지 않았습니다."
        )))
    }
}

const STALE_SELECTION_ERROR: &str = "internal.tui-selection-stale-under-action-lock";

pub(super) fn stale_selection_error() -> AppError {
    AppError::blocked(STALE_SELECTION_ERROR)
}

pub(crate) fn is_stale_selection_error(error: &AppError) -> bool {
    error.code == 3 && error.message == STALE_SELECTION_ERROR
}

pub(super) fn validate_workflow_binding(
    workflow: &state::WorkflowRecord,
    record: &ProposalRecord,
) -> Result<(), AppError> {
    if workflow.workflow_id != record.workflow_id
        || workflow.action_id != record.action_id
        || workflow.proposal_id != record.proposal_id
        || workflow.source_path != record.relative_path
        || workflow.before_hash != record.original_sha256
        || workflow.after_hash != record.proposed_sha256
        || workflow.verification_plan != record.verification_command
        || workflow.proposal_hash != record.artifact_hash
        || workflow.approval_state == "unresolved-conflict"
    {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: workflow/action/proposal/hash/verification binding이 일치하지 않습니다.",
        ));
    }
    let source = fs::read_to_string(paths::project_root().join(&record.relative_path))
        .map_err(|err| AppError::blocked(format!("approval source reread 실패: {err}")))?;
    let current_hash = sha256_text(&source);
    let allowed = current_hash == record.original_sha256
        || (matches!(
            workflow.phase.as_str(),
            "approved"
                | "pending-verification-approval"
                | "verification-approved"
                | "verification-started"
                | "verified"
                | "complete"
        ) && current_hash == record.proposed_sha256);
    if !allowed {
        return Err(AppError::blocked(format!(
            "patch approve 차단\n- 이유: 대상 파일이 preview 이후 변경되었습니다.\n- expected original sha256: {}\n- current sha256: {}",
            record.original_sha256, current_hash
        )));
    }
    Ok(())
}

pub(super) fn load_validated_approval_workflow(
    record: &ProposalRecord,
    token: &str,
    active_workflow_id: Option<&str>,
) -> Result<state::WorkflowRecord, AppError> {
    let workflow = state::load_workflow(&record.workflow_id)?;
    if !workflow.is_terminal() && active_workflow_id != Some(record.workflow_id.as_str()) {
        return Err(AppError::blocked(
            "patch approve 차단\n- 이유: current pointer/active workflow가 proposal workflow와 일치하지 않습니다.",
        ));
    }
    validate_workflow_binding(&workflow, record)?;
    validate_token_hash(&workflow.approval_credential_hash, token, record)?;
    Ok(workflow)
}

pub(super) fn success_report(workflow: &state::WorkflowRecord) -> String {
    format!(
        "패치 작업 완료\n- 결과: 성공\n- workflow id: {}\n- action id: {}\n- proposal id: {}\n- 적용 파일: {}\n- 적용 SHA-256: {}\n- 검증: 통과 ({})\n- evidence id: {}\n- stop gate: 통과\n- 미해결 승인: 없음",
        workflow.workflow_id,
        workflow.action_id,
        workflow.proposal_id,
        workflow.source_path,
        workflow.after_hash,
        ledger::redact_text(&workflow.verification_plan),
        workflow.evidence_id
    )
}

pub(super) fn failure_report(workflow: &state::WorkflowRecord) -> String {
    format!(
        "패치 작업 실패\n- 결과: 실패\n- workflow id: {}\n- proposal id: {}\n- 이유: {}\n- 성공 보고: 차단",
        workflow.workflow_id,
        display_none(&workflow.proposal_id),
        display_none(&workflow.failure_reason)
    )
}
