mod conversation;
mod intent;
mod model;
mod progress;
mod read;
mod selection;
mod status;

pub(crate) use conversation::{
    TuiAttachment, TuiAttachmentKind, TuiConversationRole, TuiConversationTurn, TuiSessionOption,
    TuiSessionTransition,
};
pub(crate) use intent::{new_tui_intent_id, OneShotSecret, TuiGateKind, TuiIntent};
pub(crate) use model::{TuiModelOption, TuiModelReadiness};
pub(crate) use progress::{TuiRequestProgress, TuiRequestProgressReporter};
pub(crate) use read::{
    TuiFreshness, TuiReadAuthority, TuiReadBudget, TuiReadContinuation, TuiReadPage, TuiReadRequest,
};
#[cfg(test)]
pub(crate) use read::{TUI_MAX_CHARS, TUI_MAX_ITEMS};
pub(crate) use selection::{
    lease_matches_active_workflow, lease_matches_terminal_selection, ObservedWorkflow,
    SelectionLease, SelectionObservation,
};
pub(crate) use status::{TuiBackendStatus, TuiStatusSnapshot, TuiVisionStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TuiWebSourceOption {
    pub(crate) source_id: String,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) opened: bool,
    pub(crate) current: bool,
}

#[cfg(test)]
#[path = "runtime_bridge/tests.rs"]
mod tests;
