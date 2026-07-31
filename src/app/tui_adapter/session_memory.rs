//! Canonical, session-scoped conversation memory for the interactive TUI.
//!
//! The controller owns only render state. This service owns durable dialogue
//! history, pair integrity, and append-only reset boundaries.

use crate::app::web_search_adapter::WebGroundingEvidence;
use crate::app::workflow_adapter::{ledger, transcript};
use crate::foundation::error::AppError;
#[cfg(test)]
use crate::surfaces::tui::runtime_bridge::TuiConversationRole;
use crate::surfaces::tui::runtime_bridge::TuiConversationTurn;

mod event_codec;
mod recording;
mod restoration;
mod tool_activity;

pub(super) use recording::{clear, record_tool_activities};
use restoration::load_for_session;
pub(super) use tool_activity::{
    ConversationToolActivity, ConversationToolName, ConversationToolStatus,
};

const CONVERSATION_STREAM_ID: &str = "tui-conversation";
const RESET_MARKER: &str = "tui conversation reset boundary";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConversationMemory {
    pub(super) turns: Vec<TuiConversationTurn>,
    web_grounding: Vec<WebGroundingEvidence>,
    tool_activities: Vec<ConversationToolActivity>,
    session_id: String,
    head_record_id: Option<String>,
}

impl ConversationMemory {
    fn empty(session_id: &str) -> Self {
        Self {
            turns: Vec::new(),
            web_grounding: Vec::new(),
            tool_activities: Vec::new(),
            session_id: session_id.to_string(),
            head_record_id: None,
        }
    }

    pub(super) fn belongs_to(&self, session_id: &str) -> bool {
        self.session_id == session_id
    }

    pub(super) fn turns(&self) -> &[TuiConversationTurn] {
        &self.turns
    }

    pub(super) fn web_grounding(&self) -> &[WebGroundingEvidence] {
        &self.web_grounding
    }

    #[cfg(test)]
    pub(super) fn tool_activities(&self) -> &[ConversationToolActivity] {
        &self.tool_activities
    }
}

pub(super) fn load() -> Result<ConversationMemory, AppError> {
    let identity = ledger::validated_current_identity()?;
    load_for_session(&identity.session_id)
}

#[cfg(test)]
pub(super) fn record_exchange(
    memory: &mut ConversationMemory,
    user_request: &str,
    assistant_response: &str,
    web_grounding: &[WebGroundingEvidence],
) -> Result<(), AppError> {
    recording::record_exchange(memory, user_request, assistant_response, web_grounding, &[])
}

pub(super) fn record_exchange_with_tool_activities(
    memory: &mut ConversationMemory,
    user_request: &str,
    assistant_response: &str,
    web_grounding: &[WebGroundingEvidence],
    tool_activities: &[ConversationToolActivity],
) -> Result<(), AppError> {
    recording::record_exchange(
        memory,
        user_request,
        assistant_response,
        web_grounding,
        tool_activities,
    )
}

#[cfg(test)]
pub(super) fn record_failure(
    memory: &mut ConversationMemory,
    user_request: &str,
    runtime_error: &str,
) -> Result<(), AppError> {
    recording::record_failure(memory, user_request, runtime_error, &[])
}

pub(super) fn record_failure_with_tool_activities(
    memory: &mut ConversationMemory,
    user_request: &str,
    runtime_error: &str,
    tool_activities: &[ConversationToolActivity],
) -> Result<(), AppError> {
    recording::record_failure(memory, user_request, runtime_error, tool_activities)
}

fn transcript_owner(identity: &ledger::RuntimeIdentity) -> transcript::TranscriptOwner {
    transcript::TranscriptOwner {
        project_id: identity.project_id.clone(),
        session_id: identity.session_id.clone(),
        stream_id: CONVERSATION_STREAM_ID.to_string(),
    }
}

#[cfg(test)]
#[path = "session_memory/tests.rs"]
mod tests;
