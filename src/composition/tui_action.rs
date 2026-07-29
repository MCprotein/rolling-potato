use crate::foundation::error::AppError;
use crate::surfaces::tui::outcome::TuiOutcome;
use crate::surfaces::tui::runtime_bridge::TuiIntent;

mod outcome;
mod port;
mod selection;
mod session;
mod workflow;

pub(crate) use port::{TuiActionPort, TuiMutationFailure};
pub(crate) use selection::{gate_descriptor, selection_lease};

pub(crate) fn dispatch_intent(
    port: &mut impl TuiActionPort,
    intent: TuiIntent,
) -> Result<TuiOutcome, AppError> {
    match intent {
        TuiIntent::Refresh { .. } | TuiIntent::Inspect { .. } => Err(AppError::usage(
            "TUI read intent는 read_tui_page 경계를 사용해야 합니다.",
        )),
        TuiIntent::ApprovePatch {
            intent_id,
            proposal_id,
            lease,
            secret,
        } => workflow::approve_patch(port, intent_id, proposal_id, lease, secret),
        TuiIntent::ApproveVerification {
            intent_id,
            proposal_id,
            lease,
            secret,
        } => workflow::approve_verification(port, intent_id, proposal_id, lease, secret),
        TuiIntent::DenyPendingGate {
            intent_id,
            workflow_id,
            gate_id,
            gate_kind,
            lease,
        } => workflow::deny_pending_gate(port, intent_id, workflow_id, gate_id, gate_kind, lease),
        TuiIntent::ResumeWorkflow {
            intent_id,
            workflow_id,
            lease,
        } => workflow::resume(port, intent_id, workflow_id, lease),
        TuiIntent::CancelWorkflow {
            intent_id,
            workflow_id,
            lease,
        } => workflow::cancel(port, intent_id, workflow_id, lease),
        TuiIntent::SelectSession {
            intent_id,
            session_id,
            lease,
        }
        | TuiIntent::ResumeSession {
            intent_id,
            session_id,
            lease,
        } => session::resume(port, intent_id, session_id, lease),
    }
}

#[cfg(test)]
#[path = "tui_action/tests.rs"]
mod tests;
