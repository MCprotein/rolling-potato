//! Interactive request routing for the canonical TUI conversation.

use super::super::session_memory::ConversationToolActivity;
use super::super::{conversation, TuiRuntimeAdapter};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::surfaces::tui::runtime_bridge::{
    TuiAttachment, TuiConversationTurn, TuiRequestProgress, TuiRequestProgressReporter,
};

mod routing;
mod support;

pub(super) struct RequestExecution {
    pub(super) response: String,
    pub(super) web_grounding: Vec<crate::app::web_search_adapter::WebGroundingEvidence>,
}

pub(super) struct RequestContext<'a> {
    pub(super) request: &'a str,
    pub(super) attachments: &'a [TuiAttachment],
    pub(super) history: &'a [TuiConversationTurn],
    pub(super) tool_history: &'a [ConversationToolActivity],
    pub(super) web_grounding: &'a [crate::app::web_search_adapter::WebGroundingEvidence],
    pub(super) progress: &'a TuiRequestProgressReporter,
    pub(super) cancellation: &'a RequestCancellationToken,
}

pub(super) fn execute(
    adapter: &mut TuiRuntimeAdapter,
    context: RequestContext<'_>,
    tool_activities: &mut Vec<ConversationToolActivity>,
) -> Result<RequestExecution, AppError> {
    context.cancellation.check()?;
    context.progress.emit(TuiRequestProgress::Preparing);
    let mut execution = routing::execute_routed(adapter, &context, tool_activities)?;
    context.cancellation.check()?;
    execution.response = conversation::ensure_public_answer(execution.response)?;
    Ok(execution)
}
