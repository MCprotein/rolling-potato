use crate::app::web_search_adapter::WebGroundingEvidence;
use crate::app::workflow_adapter::transcript;
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

use super::event_codec::{parse_conversation_event, ConversationEvent};
use super::{ConversationMemory, CONVERSATION_STREAM_ID, RESET_MARKER};

const MAX_WEB_GROUNDING_SOURCES: usize = 12;

pub(super) fn load_for_session(session_id: &str) -> Result<ConversationMemory, AppError> {
    let records = transcript::records_for_session(session_id)?;
    let mut memory = ConversationMemory::empty(session_id);
    let mut pending_user: Option<TuiConversationTurn> = None;

    for record in records
        .into_iter()
        .filter(|record| record.workflow_id == CONVERSATION_STREAM_ID)
    {
        match record.kind.as_str() {
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
            "evidence" => {
                restore_evidence(&mut memory, &mut pending_user, &record.content);
                memory.head_record_id = Some(record.record_id);
            }
            _ => {}
        }
    }
    Ok(memory)
}

fn restore_evidence(
    memory: &mut ConversationMemory,
    pending_user: &mut Option<TuiConversationTurn>,
    content: &str,
) {
    match parse_conversation_event(content) {
        Some(ConversationEvent::Reset) => reset(memory, pending_user),
        Some(ConversationEvent::RuntimeError(content)) => {
            push_response(memory, pending_user, TuiConversationRole::Error, content);
        }
        Some(ConversationEvent::WebGrounding(evidence)) => {
            push_web_grounding(&mut memory.web_grounding, evidence);
        }
        None if content == RESET_MARKER => reset(memory, pending_user),
        None => push_response(
            memory,
            pending_user,
            TuiConversationRole::Error,
            content.to_string(),
        ),
    }
}

fn reset(memory: &mut ConversationMemory, pending_user: &mut Option<TuiConversationTurn>) {
    memory.turns.clear();
    memory.web_grounding.clear();
    *pending_user = None;
}

fn push_response(
    memory: &mut ConversationMemory,
    pending_user: &mut Option<TuiConversationTurn>,
    role: TuiConversationRole,
    content: String,
) {
    if let Some(user) = pending_user.take() {
        memory.turns.push(user);
        memory.turns.push(TuiConversationTurn { role, content });
    }
}

pub(super) fn push_web_grounding(
    grounding: &mut Vec<WebGroundingEvidence>,
    evidence: WebGroundingEvidence,
) {
    if let Some(index) = grounding
        .iter()
        .position(|stored| stored.source_id == evidence.source_id)
    {
        grounding.remove(index);
    }
    grounding.push(evidence);
    if grounding.len() > MAX_WEB_GROUNDING_SOURCES {
        grounding.drain(..grounding.len() - MAX_WEB_GROUNDING_SOURCES);
    }
}
