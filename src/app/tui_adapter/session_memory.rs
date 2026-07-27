//! Canonical, session-scoped conversation memory for the interactive TUI.
//!
//! The controller owns only render state. This service owns durable dialogue
//! history, pair integrity, and append-only reset boundaries.

use crate::app::web_search_adapter::WebGroundingEvidence;
use crate::app::workflow_adapter::{ledger, state, transcript};
use crate::foundation::error::AppError;
use crate::foundation::serialization::{self as strict_json, CanonicalObject, CanonicalValue};
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

const CONVERSATION_STREAM_ID: &str = "tui-conversation";
const RESET_MARKER: &str = "tui conversation reset boundary";
const CONVERSATION_EVENT_SCHEMA_VERSION: u64 = 1;
const MAX_RUNTIME_ERROR_CHARS: usize = 2_048;
const MAX_WEB_GROUNDING_SOURCES: usize = 12;
const MAX_WEB_GROUNDING_EXCERPT_CHARS: usize = 1_536;

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

    pub(super) fn prompt_history(&self) -> Vec<TuiConversationTurn> {
        self.turns.clone()
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

fn load_for_session(session_id: &str) -> Result<ConversationMemory, AppError> {
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
                match parse_conversation_event(&record.content) {
                    Some(ConversationEvent::Reset) => {
                        memory.turns.clear();
                        memory.web_grounding.clear();
                        pending_user = None;
                    }
                    Some(ConversationEvent::RuntimeError(content)) => {
                        if let Some(user) = pending_user.take() {
                            memory.turns.push(user);
                            memory.turns.push(TuiConversationTurn {
                                role: TuiConversationRole::Error,
                                content,
                            });
                        }
                    }
                    Some(ConversationEvent::WebGrounding(evidence)) => {
                        push_web_grounding(&mut memory.web_grounding, evidence);
                    }
                    None if record.content == RESET_MARKER => {
                        memory.turns.clear();
                        memory.web_grounding.clear();
                        pending_user = None;
                    }
                    None => {
                        if let Some(user) = pending_user.take() {
                            memory.turns.push(user);
                            memory.turns.push(TuiConversationTurn {
                                role: TuiConversationRole::Error,
                                content: record.content,
                            });
                        }
                    }
                }
                memory.head_record_id = Some(record.record_id);
            }
            _ => {}
        }
    }
    Ok(memory)
}

enum ConversationEvent {
    Reset,
    RuntimeError(String),
    WebGrounding(WebGroundingEvidence),
}

fn render_reset_event() -> String {
    render_event(vec![string_entry("event_type", "reset")])
}

fn render_runtime_error_event(content: &str) -> String {
    render_event(vec![
        string_entry("event_type", "runtime_error"),
        string_entry("content", content),
    ])
}

fn render_web_grounding_event(evidence: &WebGroundingEvidence) -> String {
    render_event(vec![
        string_entry("event_type", "web_evidence"),
        string_entry("source_id", &evidence.source_id),
        string_entry("title", &evidence.title),
        string_entry("url", &evidence.url),
        string_entry(
            "excerpt",
            &evidence
                .excerpt
                .chars()
                .take(MAX_WEB_GROUNDING_EXCERPT_CHARS)
                .collect::<String>(),
        ),
    ])
}

fn render_event(mut entries: Vec<(String, CanonicalValue)>) -> String {
    entries.insert(
        0,
        (
            "schema_version".to_string(),
            CanonicalValue::Unsigned {
                raw: CONVERSATION_EVENT_SCHEMA_VERSION.to_string(),
            },
        ),
    );
    strict_json::render_canonical_object(&CanonicalObject { entries })
}

fn string_entry(key: &str, value: &str) -> (String, CanonicalValue) {
    (key.to_string(), CanonicalValue::String(value.to_string()))
}

fn parse_conversation_event(content: &str) -> Option<ConversationEvent> {
    let object = strict_json::parse_object(
        content,
        &[
            "schema_version",
            "event_type",
            "content",
            "source_id",
            "title",
            "url",
            "excerpt",
        ],
        "conversation event",
    )
    .ok()?;
    if strict_json::number(&object, "schema_version", "conversation event").ok()?
        != CONVERSATION_EVENT_SCHEMA_VERSION
    {
        return None;
    }
    match strict_json::string(&object, "event_type", "conversation event")
        .ok()?
        .as_str()
    {
        "reset" => Some(ConversationEvent::Reset),
        "runtime_error" => Some(ConversationEvent::RuntimeError(
            strict_json::string(&object, "content", "conversation event").ok()?,
        )),
        "web_evidence" => Some(ConversationEvent::WebGrounding(WebGroundingEvidence {
            source_id: strict_json::string(&object, "source_id", "conversation event").ok()?,
            title: strict_json::string(&object, "title", "conversation event").ok()?,
            url: strict_json::string(&object, "url", "conversation event").ok()?,
            excerpt: strict_json::string(&object, "excerpt", "conversation event").ok()?,
        })),
        _ => None,
    }
}

fn push_web_grounding(grounding: &mut Vec<WebGroundingEvidence>, evidence: WebGroundingEvidence) {
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
