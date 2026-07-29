use crate::foundation::error::AppError;
use crate::surfaces::tui::outcome::validate_tui_id;
use crate::surfaces::tui::runtime_bridge::{SelectionLease, TuiGateKind};

use super::port::TuiActionPort;

pub(crate) fn selection_lease(
    port: &mut impl TuiActionPort,
    selected_object_id: &str,
) -> Result<SelectionLease, AppError> {
    validate_tui_id(selected_object_id, "selected object")?;
    Ok(port.selection_observation()?.lease_for(selected_object_id))
}

pub(crate) fn gate_descriptor(
    port: &mut impl TuiActionPort,
    workflow_id: &str,
) -> Result<(String, TuiGateKind), AppError> {
    let workflow = port.workflow(workflow_id)?;
    let kind = match (workflow.phase.as_str(), workflow.failure_reason.as_str()) {
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
    Ok((workflow.proposal_id, kind))
}
