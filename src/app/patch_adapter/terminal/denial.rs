use super::super::*;
use super::gates::{
    denial_phase_outcome_code, terminal_action_receipt_exists, validate_stored_terminal_gate,
    validate_terminal_gate,
};
use super::rollback::{load_bound_proposal, prepare_terminal_rollback_source};

#[cfg(test)]
pub fn deny_pending_gate(workflow_id: &str, intent_id: &str) -> Result<TuiOutcome, AppError> {
    deny_pending_gate_transaction(workflow_id, intent_id, None)
}

pub(crate) fn deny_pending_gate_for_tui(
    workflow_id: &str,
    intent_id: &str,
    gate_id: &str,
    gate_kind: TuiGateKind,
    lease: &SelectionLease,
) -> Result<TuiOutcome, AppError> {
    deny_pending_gate_transaction(workflow_id, intent_id, Some((gate_id, gate_kind, lease)))
}

fn deny_pending_gate_transaction(
    workflow_id: &str,
    intent_id: &str,
    expected: Option<(&str, TuiGateKind, &SelectionLease)>,
) -> Result<TuiOutcome, AppError> {
    validate_outcome_id(intent_id, "intent")?;
    let (observed, _approval_lock) = load_workflow_under_approval_lock(workflow_id)?;
    validate_outcome_id(&observed.workflow_id, "workflow")?;
    if observed.phase == "cancelled"
        && observed.failure_reason == "user-denied-patch"
        && terminal_action_receipt_exists(intent_id, workflow_id, "patch.apply.denied")?
    {
        validate_stored_terminal_gate(&observed, expected, TuiGateKind::PatchApply)?;
        return deny_patch_accepted(intent_id, &observed.workflow_id);
    }
    if observed.phase == "cancelled"
        && observed.failure_reason == "user-denied-verification"
        && terminal_action_receipt_exists(intent_id, workflow_id, "patch.verification.denied")?
    {
        validate_stored_terminal_gate(&observed, expected, TuiGateKind::VerificationCommand)?;
        return deny_verification_accepted(intent_id, &observed.workflow_id);
    }
    let identity = ledger::validated_current_identity()?;
    let transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::Cancel,
    )?;
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(workflow_id)?;
    let workflow = workflow_guard.load_current()?;
    if workflow != observed {
        return Err(stale_selection_error());
    }
    if workflow.is_terminal() {
        if let Some((gate_id, gate_kind, lease)) = expected {
            if !state::tui_lease_matches_terminal_selection_under_transition(lease, workflow_id)? {
                return Err(stale_selection_error());
            }
            validate_terminal_gate(&workflow, gate_id, gate_kind)?;
        }
        return exact_tui_outcome(
            TuiOutcomeCode::DenyBlockedTerminalState,
            TuiOutcomeContext {
                intent_id: Some(intent_id),
                workflow_id: Some(&workflow.workflow_id),
                phase: Some(&workflow.phase),
                ..TuiOutcomeContext::default()
            },
        );
    }
    if let Some((_, _, lease)) = expected {
        if !state::tui_lease_matches_workflow_under_transition(lease, workflow_id)? {
            return Err(stale_selection_error());
        }
    }
    if let Some((gate_id, gate_kind, _)) = expected {
        validate_terminal_gate(&workflow, gate_id, gate_kind)?;
    }
    match denial_phase_outcome_code(&workflow.phase) {
        Some(TuiOutcomeCode::DenyPatchAccepted) => {
            let mut terminal = workflow.clone();
            terminal.phase = "cancelled".to_string();
            terminal.failure_reason = "user-denied-patch".to_string();
            terminal.approval_state = "denied".to_string();
            terminal.verification_approval_state = "not-issued".to_string();
            if let Some(mut skill_runtime) = workflow_skill_runtime(&terminal)? {
                skill_runtime.transition(skill::SkillState::Cancelled)?;
                skill_runtime.store_in_workflow(&mut terminal);
            }
            let committed = state::transition_project_current_state_prepared_terminal_action(
                &transition_guard,
                &workflow_guard,
                state::TerminalActionRequest {
                    intent_id,
                    intent_kind: "deny-patch",
                    identity: &identity,
                    before: &workflow,
                    terminal,
                    audit_event_type: "patch.apply.denied",
                    audit_summary: "patch apply approval denied",
                    audit_details: "gate=patch-apply effect=none",
                    source: None,
                },
            )?;
            deny_patch_accepted(intent_id, &committed.workflow_id)
        }
        Some(TuiOutcomeCode::DenyVerificationRolledBack) => {
            let record = load_bound_proposal(&workflow)?;
            let source = match prepare_terminal_rollback_source(&record, intent_id, true) {
                Ok(Some(source)) => source,
                Ok(None) => {
                    return Err(AppError::blocked(
                        "prepared verification denial rollback receipt 누락",
                    ))
                }
                Err(error) if error.message.starts_with("internal.rollback-conflict:") => {
                    return exact_tui_outcome(
                        TuiOutcomeCode::RollbackConflict,
                        TuiOutcomeContext {
                            intent_id: Some(intent_id),
                            workflow_id: Some(&workflow.workflow_id),
                            ..TuiOutcomeContext::default()
                        },
                    )
                }
                Err(error) => return Err(error),
            };
            let mut terminal = workflow.clone();
            terminal.phase = "cancelled".to_string();
            terminal.failure_reason = "user-denied-verification".to_string();
            terminal.approval_state = "applied-then-rolled-back".to_string();
            terminal.verification_approval_state = "denied".to_string();
            if let Some(mut skill_runtime) = workflow_skill_runtime(&terminal)? {
                skill_runtime.transition(skill::SkillState::Cancelled)?;
                skill_runtime.store_in_workflow(&mut terminal);
            }
            let committed = state::transition_project_current_state_prepared_terminal_action(
                &transition_guard,
                &workflow_guard,
                state::TerminalActionRequest {
                    intent_id,
                    intent_kind: "deny-verification",
                    identity: &identity,
                    before: &workflow,
                    terminal,
                    audit_event_type: "patch.verification.denied",
                    audit_summary: "verification approval denied and source rolled back",
                    audit_details: "gate=verification-command rollback=restored",
                    source: Some(source),
                },
            )?;
            deny_verification_accepted(intent_id, &committed.workflow_id)
        }
        Some(TuiOutcomeCode::DenyBlockedNotPending) => exact_tui_outcome(
            TuiOutcomeCode::DenyBlockedNotPending,
            TuiOutcomeContext {
                intent_id: Some(intent_id),
                workflow_id: Some(&workflow.workflow_id),
                phase: Some(&workflow.phase),
                ..TuiOutcomeContext::default()
            },
        ),
        Some(TuiOutcomeCode::DenyBlockedTerminalState) => exact_tui_outcome(
            TuiOutcomeCode::DenyBlockedTerminalState,
            TuiOutcomeContext {
                intent_id: Some(intent_id),
                workflow_id: Some(&workflow.workflow_id),
                phase: Some(&workflow.phase),
                ..TuiOutcomeContext::default()
            },
        ),
        Some(other) => Err(AppError::blocked(format!(
            "승인 거부 차단\n- code: deny.corrupt-state\n- mapped outcome: {}\n- 동작: 허용되지 않은 denial outcome을 실행하지 않았습니다.",
            other.as_str()
        ))),
        None => Err(AppError::blocked(
            "승인 거부 차단\n- code: deny.corrupt-state\n- 동작: 알 수 없는 workflow phase를 출력하거나 변경하지 않았습니다.",
        )),
    }
}

fn deny_patch_accepted(intent_id: &str, workflow_id: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::DenyPatchAccepted,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            workflow_id: Some(workflow_id),
            ..TuiOutcomeContext::default()
        },
    )
}

fn deny_verification_accepted(intent_id: &str, workflow_id: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::DenyVerificationRolledBack,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            workflow_id: Some(workflow_id),
            ..TuiOutcomeContext::default()
        },
    )
}
