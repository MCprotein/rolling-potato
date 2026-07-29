use crate::foundation::error::AppError;
use crate::surfaces::tui::outcome::{
    exact_tui_outcome, validate_tui_id, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
};
use crate::surfaces::tui::runtime_bridge::SelectionLease;

use super::outcome::stale_selection;
use super::port::TuiActionPort;

pub(super) fn resume(
    port: &mut impl TuiActionPort,
    intent_id: String,
    session_id: String,
    lease: SelectionLease,
) -> Result<TuiOutcome, AppError> {
    validate_tui_id(&intent_id, "intent")?;
    if port
        .resume_session(&session_id, &intent_id, &lease)?
        .is_none()
    {
        return stale_selection(&session_id);
    }
    exact_tui_outcome(
        TuiOutcomeCode::ResumeAccepted,
        TuiOutcomeContext {
            intent_id: Some(&intent_id),
            workflow_id: Some(&session_id),
            ..TuiOutcomeContext::default()
        },
    )
}
