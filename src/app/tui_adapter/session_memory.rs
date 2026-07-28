//! Canonical, session-scoped conversation memory for the interactive TUI.
//!
//! The controller owns only render state. This service owns durable dialogue
//! history, pair integrity, and append-only reset boundaries.

use crate::app::web_search_adapter::WebGroundingEvidence;
use crate::app::workflow_adapter::{ledger, state, transcript};
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

mod event_codec;
mod restoration;

use event_codec::{render_reset_event, render_runtime_error_event, render_web_grounding_event};
use restoration::{load_for_session, push_web_grounding};

const CONVERSATION_STREAM_ID: &str = "tui-conversation";
const RESET_MARKER: &str = "tui conversation reset boundary";
const MAX_RUNTIME_ERROR_CHARS: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConversationMemory {
    pub(super) turns: Vec<TuiConversationTurn>,
    web_grounding: Vec<WebGroundingEvidence>,
    session_id: String,
    head_record_id: Option<String>,
}

impl ConversationMemory {
    fn empty(session_id: &str) -> Self {
        Self {
            turns: Vec::new(),
            web_grounding: Vec::new(),
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
}

pub(super) fn load() -> Result<ConversationMemory, AppError> {
    let identity = ledger::validated_current_identity()?;
    load_for_session(&identity.session_id)
}

pub(super) fn record_exchange(
    memory: &mut ConversationMemory,
    user_request: &str,
    assistant_response: &str,
    web_grounding: &[WebGroundingEvidence],
) -> Result<(), AppError> {
    record_result(
        memory,
        user_request,
        assistant_response,
        "model",
        TuiConversationRole::Assistant,
        web_grounding,
    )
}

pub(super) fn record_failure(
    memory: &mut ConversationMemory,
    user_request: &str,
    runtime_error: &str,
) -> Result<(), AppError> {
    let bounded_error = runtime_error
        .chars()
        .take(MAX_RUNTIME_ERROR_CHARS)
        .collect::<String>();
    record_result(
        memory,
        user_request,
        &bounded_error,
        "evidence",
        TuiConversationRole::Error,
        &[],
    )
}

fn record_result(
    memory: &mut ConversationMemory,
    user_request: &str,
    response: &str,
    response_kind: &str,
    response_role: TuiConversationRole,
    web_grounding: &[WebGroundingEvidence],
) -> Result<(), AppError> {
    let identity = ledger::validated_current_identity()?;
    if !memory.belongs_to(&identity.session_id) {
        return Err(AppError::blocked(
            "conversation memory session binding이 현재 session과 일치하지 않습니다.",
        ));
    }
    let owner = transcript_owner(&identity);
    let exchange_id = exchange_id(
        &owner,
        memory.head_record_id.as_deref(),
        user_request,
        &crate::surfaces::tui::runtime_bridge::new_tui_intent_id(),
    );
    let user = transcript::record_session_turn(
        &owner,
        "user",
        &format!("{exchange_id}-user"),
        user_request,
        &[],
    )?;
    memory.head_record_id = Some(user.record_id);
    let persisted_response = if response_role == TuiConversationRole::Error {
        render_runtime_error_event(response)
    } else {
        response.to_string()
    };
    let result = transcript::record_session_turn(
        &owner,
        response_kind,
        &format!("{exchange_id}-{response_kind}"),
        &persisted_response,
        &[],
    )?;
    let mut head_record_id = result.record_id.clone();
    for (index, evidence) in web_grounding.iter().enumerate() {
        let record = transcript::record_session_turn(
            &owner,
            "evidence",
            &format!("{exchange_id}-web-evidence-{index}"),
            &render_web_grounding_event(evidence),
            &[],
        )?;
        head_record_id = record.record_id;
    }
    memory.turns.push(TuiConversationTurn {
        role: TuiConversationRole::User,
        content: user_request.to_string(),
    });
    memory.turns.push(TuiConversationTurn {
        role: response_role,
        content: response.to_string(),
    });
    for evidence in web_grounding {
        push_web_grounding(&mut memory.web_grounding, evidence.clone());
    }
    memory.head_record_id = Some(head_record_id);
    Ok(())
}

pub(super) fn clear(memory: &mut ConversationMemory) -> Result<(), AppError> {
    let identity = ledger::validated_current_identity()?;
    if !memory.belongs_to(&identity.session_id) {
        return Err(AppError::blocked(
            "conversation memory session binding이 현재 session과 일치하지 않습니다.",
        ));
    }
    let owner = transcript_owner(&identity);
    let causal_id = format!(
        "conversation-reset-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\n{}",
            owner.project_id,
            owner.session_id,
            memory.head_record_id.as_deref().unwrap_or("root"),
            crate::surfaces::tui::runtime_bridge::new_tui_intent_id()
        ))[..24]
    );
    let reset = transcript::record_session_turn(
        &owner,
        "evidence",
        &causal_id,
        &render_reset_event(),
        &[],
    )?;
    memory.turns.clear();
    memory.web_grounding.clear();
    memory.head_record_id = Some(reset.record_id);
    Ok(())
}

fn transcript_owner(identity: &ledger::RuntimeIdentity) -> transcript::TranscriptOwner {
    transcript::TranscriptOwner {
        project_id: identity.project_id.clone(),
        session_id: identity.session_id.clone(),
        stream_id: CONVERSATION_STREAM_ID.to_string(),
    }
}

fn exchange_id(
    owner: &transcript::TranscriptOwner,
    head_record_id: Option<&str>,
    user_request: &str,
    nonce: &str,
) -> String {
    format!(
        "conversation-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\n{}\n{}",
            owner.project_id,
            owner.session_id,
            head_record_id.unwrap_or("root"),
            user_request,
            nonce
        ))[..24]
    )
}

#[cfg(test)]
#[path = "session_memory/tests.rs"]
mod tests;
