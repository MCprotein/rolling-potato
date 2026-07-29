use super::super::*;

pub(super) fn workflow_has_applied_source(workflow: &state::WorkflowRecord) -> bool {
    matches!(
        workflow.phase.as_str(),
        "approved"
            | "pending-verification-approval"
            | "verification-approved"
            | "verification-started"
            | "verified"
    ) || matches!(
        workflow.approval_state.as_str(),
        "applied" | "approved" | "applied-then-rolled-back"
    )
}

pub(super) fn load_bound_proposal(
    workflow: &state::WorkflowRecord,
) -> Result<ProposalRecord, AppError> {
    let proposal_path =
        paths::project_patch_proposals_dir().join(format!("{}.txt", workflow.proposal_id));
    let record = load_proposal_record(&workflow.proposal_id, &proposal_path)?;
    validate_workflow_binding(workflow, &record)?;
    Ok(record)
}

pub(super) fn prepare_terminal_rollback_source(
    record: &ProposalRecord,
    intent_id: &str,
    require_receipt: bool,
) -> Result<Option<state::PreparedTerminalSource>, AppError> {
    let target = resolve_target_for("terminal rollback", &record.relative_path)?;
    let current = fs::read(&target.absolute_path)
        .map_err(|err| AppError::blocked(format!("terminal rollback target read 실패: {err}")))?;
    let current_hash = sha256_bytes(&current);
    if current_hash != record.proposed_sha256 && current_hash != record.original_sha256 {
        return Err(AppError::blocked(format!(
            "internal.rollback-conflict:target-sha256={current_hash}"
        )));
    }
    if current_hash == record.original_sha256 && !require_receipt {
        return Ok(None);
    }
    let rollback_path = rollback_path_for_record(record)?;
    let original = fs::read(&rollback_path)
        .map_err(|err| AppError::blocked(format!("terminal rollback record read 실패: {err}")))?;
    if sha256_bytes(&original) != record.original_sha256 {
        return Err(AppError::blocked(
            "internal.rollback-conflict:rollback-record-hash",
        ));
    }
    let plan = transition::prepare_source_install_v1(
        intent_id,
        &record.proposal_id,
        &target.absolute_path,
        &current,
        &original,
    )?;
    Ok(Some(state::PreparedTerminalSource {
        plan,
        before: current,
        proposed: original,
    }))
}
