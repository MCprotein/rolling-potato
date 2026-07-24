//! Canonical, session-scoped conversation memory for the interactive TUI.
//!
//! The controller owns only render state. This service owns durable dialogue
//! history, pair integrity, and append-only reset boundaries.

use crate::app::workflow_adapter::{ledger, state, transcript};
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

const CONVERSATION_STREAM_ID: &str = "tui-conversation";
const RESET_MARKER: &str = "tui conversation reset boundary";
const MAX_PROMPT_HISTORY_TURNS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConversationMemory {
    pub(super) turns: Vec<TuiConversationTurn>,
    session_id: String,
    head_record_id: Option<String>,
}

impl ConversationMemory {
    fn empty(session_id: &str) -> Self {
        Self {
            turns: Vec::new(),
            session_id: session_id.to_string(),
            head_record_id: None,
        }
    }

    pub(super) fn belongs_to(&self, session_id: &str) -> bool {
        self.session_id == session_id
    }

    pub(super) fn prompt_history(&self) -> Vec<TuiConversationTurn> {
        let start = self.turns.len().saturating_sub(MAX_PROMPT_HISTORY_TURNS);
        self.turns[start..].to_vec()
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
    let model = transcript::record_session_turn(
        &owner,
        "model",
        &format!("{exchange_id}-model"),
        assistant_response,
        &[],
    )?;
    memory.turns.push(TuiConversationTurn {
        role: TuiConversationRole::User,
        content: user_request.to_string(),
    });
    memory.turns.push(TuiConversationTurn {
        role: TuiConversationRole::Assistant,
        content: assistant_response.to_string(),
    });
    memory.head_record_id = Some(model.record_id);
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
    let reset = transcript::record_session_turn(&owner, "evidence", &causal_id, RESET_MARKER, &[])?;
    memory.turns.clear();
    memory.head_record_id = Some(reset.record_id);
    Ok(())
}

fn load_for_session(session_id: &str) -> Result<ConversationMemory, AppError> {
    let records = transcript::records_for_session(session_id)?;
    let mut memory = ConversationMemory::empty(session_id);
    let mut pending_user: Option<TuiConversationTurn> = None;

    for record in records
        .into_iter()
        .filter(|record| record.workflow_id == CONVERSATION_STREAM_ID)
    {
        match record.kind.as_str() {
            "evidence" if record.content == RESET_MARKER => {
                memory.turns.clear();
                pending_user = None;
                memory.head_record_id = Some(record.record_id);
            }
            "user" => {
                pending_user = Some(TuiConversationTurn {
                    role: TuiConversationRole::User,
                    content: record.content,
                });
                memory.head_record_id = Some(record.record_id);
            }
            "model" => {
                if let Some(user) = pending_user.take() {
                    memory.turns.push(user);
                    memory.turns.push(TuiConversationTurn {
                        role: TuiConversationRole::Assistant,
                        content: record.content,
                    });
                }
                memory.head_record_id = Some(record.record_id);
            }
            _ => {}
        }
    }
    Ok(memory)
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
