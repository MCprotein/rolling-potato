use super::super::*;
use super::rollback::{
    load_bound_proposal, prepare_terminal_rollback_source, workflow_has_applied_source,
};

pub fn cancel_workflow_report(workflow_id: &str) -> Result<String, AppError> {
    let intent_id = format!("intent-cancel-{}", workflow_id);
    let workflow = cancel_workflow_transaction(workflow_id, &intent_id, None).map_err(|error| {
        if let Some(reason) = error.message.strip_prefix("internal.rollback-conflict:") {
            AppError::blocked(format!(
                "workflow cancel 차단\n- 이유: 적용된 source를 안전하게 복원하지 못했습니다.\n- rollback: {reason}\n- pointer: 유지"
            ))
        } else if let Some(phase) = error.message.strip_prefix("internal.cancel-terminal:") {
            AppError::blocked(format!(
                "cancel 차단\n- 이유: terminal workflow는 취소할 수 없습니다.\n- phase: {phase}"
            ))
        } else {
            error
        }
    })?;
    Ok(format!(
        "workflow 취소 완료\n- workflow id: {}\n- phase: cancelled\n- source 복원: 검증됨 또는 적용 전\n- backend/verification 재실행: 없음",
        workflow.workflow_id
    ))
}

pub(crate) fn cancel_workflow_for_tui(
    workflow_id: &str,
    intent_id: &str,
    lease: &SelectionLease,
) -> Result<(), AppError> {
    cancel_workflow_transaction(workflow_id, intent_id, Some(lease)).map(|_| ())
}

fn cancel_workflow_transaction(
    workflow_id: &str,
    intent_id: &str,
    expected_lease: Option<&SelectionLease>,
) -> Result<state::WorkflowRecord, AppError> {
    validate_outcome_id(intent_id, "intent")?;
    let (observed, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    if observed.phase == "complete" {
        return Err(AppError::blocked(format!(
            "internal.cancel-terminal:{}",
            observed.phase
        )));
    }
    if matches!(observed.phase.as_str(), "failed" | "cancelled") {
        return Err(AppError::blocked(format!(
            "internal.cancel-terminal:{}",
            observed.phase
        )));
    }
    let identity = ledger::validated_current_identity()?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::Cancel,
    )?;
    if let Some(lease) = expected_lease {
        if !state::tui_lease_matches_workflow_under_transition(lease, workflow_id)? {
            return Err(stale_selection_error());
        }
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(workflow_id)?;
    let current = workflow_guard.load_current()?;
    if current != observed {
        return Err(stale_selection_error());
    }
    let source = if workflow_has_applied_source(&current) {
        let record = load_bound_proposal(&current)?;
        prepare_terminal_rollback_source(&record, intent_id, false)?
    } else {
        None
    };
    let mut terminal = current.clone();
    terminal.phase = "cancelled".to_string();
    terminal.failure_reason = "user-cancelled".to_string();
    terminal.approval_state = "cancelled".to_string();
    terminal.verification_approval_state = "cancelled".to_string();
    if let Some(mut runtime) = workflow_skill_runtime(&terminal)? {
        runtime.transition(skill::SkillState::Cancelled)?;
        runtime.store_in_workflow(&mut terminal);
    }
    state::transition_project_current_state_prepared_terminal_action(
        &transition_guard,
        &workflow_guard,
        state::TerminalActionRequest {
            intent_id,
            intent_kind: "cancel-workflow",
            identity: &identity,
            before: &current,
            terminal,
            audit_event_type: "workflow.user-cancelled",
            audit_summary: "workflow cancelled by user",
            audit_details: "reason=user-cancelled",
            source,
        },
    )
}
