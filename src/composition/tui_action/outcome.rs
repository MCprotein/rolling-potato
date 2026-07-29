use crate::foundation::error::AppError;
use crate::surfaces::tui::outcome::{
    exact_tui_outcome, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
};

use super::port::TuiMutationFailure;

pub(super) fn stale_selection(workflow_id: &str) -> Result<TuiOutcome, AppError> {
    exact_tui_outcome(
        TuiOutcomeCode::ResumeStaleSelection,
        TuiOutcomeContext {
            workflow_id: Some(workflow_id),
            ..TuiOutcomeContext::default()
        },
    )
}

pub(super) fn secret_refresh_only(intent_id: &str) -> TuiOutcome {
    exact_tui_outcome(
        TuiOutcomeCode::SecretRefreshOnly,
        TuiOutcomeContext {
            intent_id: Some(intent_id),
            ..TuiOutcomeContext::default()
        },
    )
    .expect("validated TUI intent IDs always produce the refresh-only outcome")
}

pub(super) fn unexpected_or_other(operation: &str, failure: TuiMutationFailure) -> AppError {
    match failure {
        TuiMutationFailure::Other(error) => error,
        _ => AppError::runtime(format!("TUI mutation adapter contract 불일치: {operation}")),
    }
}
