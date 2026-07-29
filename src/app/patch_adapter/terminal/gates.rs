use super::super::*;

pub(crate) fn denial_phase_outcome_code(phase: &str) -> Option<TuiOutcomeCode> {
    match phase {
        "pending-approval" => Some(TuiOutcomeCode::DenyPatchAccepted),
        "pending-verification-approval" => Some(TuiOutcomeCode::DenyVerificationRolledBack),
        "approved" | "verification-approved" | "verification-started" | "verified" => {
            Some(TuiOutcomeCode::DenyBlockedNotPending)
        }
        "complete" | "failed" | "cancelled" => Some(TuiOutcomeCode::DenyBlockedTerminalState),
        _ => None,
    }
}

pub(super) fn validate_terminal_gate(
    workflow: &state::WorkflowRecord,
    gate_id: &str,
    gate_kind: TuiGateKind,
) -> Result<(), AppError> {
    validate_outcome_id(gate_id, "gate")?;
    let expected_kind = match (workflow.phase.as_str(), workflow.failure_reason.as_str()) {
        ("cancelled", "user-denied-patch") => TuiGateKind::PatchApply,
        ("cancelled", "user-denied-verification") => TuiGateKind::VerificationCommand,
        ("pending-approval" | "approved", _) => TuiGateKind::PatchApply,
        (
            "pending-verification-approval"
            | "verification-approved"
            | "verification-started"
            | "verified",
            _,
        ) => TuiGateKind::VerificationCommand,
        _ if matches!(
            workflow.approval_state.as_str(),
            "pending" | "pending-rotated"
        ) =>
        {
            TuiGateKind::PatchApply
        }
        _ => TuiGateKind::VerificationCommand,
    };
    if gate_id != workflow.proposal_id || gate_kind != expected_kind {
        return Err(stale_selection_error());
    }
    Ok(())
}

pub(super) fn validate_stored_terminal_gate(
    workflow: &state::WorkflowRecord,
    expected: Option<(&str, TuiGateKind, &SelectionLease)>,
    expected_kind: TuiGateKind,
) -> Result<(), AppError> {
    if let Some((gate_id, gate_kind, lease)) = expected {
        if gate_id != workflow.proposal_id
            || gate_kind != expected_kind
            || lease.selected_object_id != workflow.workflow_id
        {
            return Err(stale_selection_error());
        }
    }
    Ok(())
}

pub(super) fn terminal_action_receipt_exists(
    intent_id: &str,
    workflow_id: &str,
    event_type: &str,
) -> Result<bool, AppError> {
    ledger::event_details_match(
        event_type,
        &[("intent_id", intent_id), ("workflow_id", workflow_id)],
    )
}
