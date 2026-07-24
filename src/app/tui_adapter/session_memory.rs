//! Canonical, session-scoped conversation memory for the interactive TUI.
//!
//! The controller owns only render state. This service owns durable dialogue
//! history, pair integrity, and append-only reset boundaries.

use crate::app::workflow_adapter::{ledger, state, transcript};
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

const CONVERSATION_STREAM_ID: &str = "tui-conversation";
const RESET_MARKER: &str = "tui conversation reset boundary";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConversationMemory {
    pub(super) turns: Vec<TuiConversationTurn>,
    head_record_id: Option<String>,
}

impl ConversationMemory {
    fn empty() -> Self {
        Self {
            turns: Vec::new(),
            head_record_id: None,
        }
    }
}

pub(super) fn load() -> Result<ConversationMemory, AppError> {
    let identity = ledger::validated_current_identity()?;
    load_for_session(&identity.session_id)
}

pub(super) fn record_exchange(
    before: &ConversationMemory,
    user_request: &str,
    assistant_response: &str,
) -> Result<ConversationMemory, AppError> {
    let identity = ledger::validated_current_identity()?;
    let owner = transcript_owner(&identity);
    let exchange_id = exchange_id(&owner, before.head_record_id.as_deref(), user_request);
    transcript::record_session_turn(
        &owner,
        "user",
        &format!("{exchange_id}-user"),
        user_request,
        &[],
    )?;
    transcript::record_session_turn(
        &owner,
        "model",
        &format!("{exchange_id}-model"),
        assistant_response,
        &[],
    )?;
    load_for_session(&identity.session_id)
}

pub(super) fn clear() -> Result<(), AppError> {
    let identity = ledger::validated_current_identity()?;
    let owner = transcript_owner(&identity);
    let memory = load_for_session(&identity.session_id)?;
    let causal_id = format!(
        "conversation-reset-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}",
            owner.project_id,
            owner.session_id,
            memory.head_record_id.as_deref().unwrap_or("root")
        ))[..24]
    );
    transcript::record_session_turn(&owner, "evidence", &causal_id, RESET_MARKER, &[])?;
    Ok(())
}

fn load_for_session(session_id: &str) -> Result<ConversationMemory, AppError> {
    let records = transcript::records_for_session(session_id)?;
    let mut memory = ConversationMemory::empty();
    let mut pending_user: Option<TuiConversationTurn> = None;

    for record in records
        .into_iter()
        .filter(|record| record.workflow_id == CONVERSATION_STREAM_ID)
    {
        match record.kind.as_str() {
            "evidence" if record.content == RESET_MARKER => {
                memory = ConversationMemory::empty();
            }
            "user" => {
                pending_user = Some(TuiConversationTurn {
                    role: TuiConversationRole::User,
                    content: record.content,
                });
            }
            "model" => {
                let Some(user) = pending_user.take() else {
                    continue;
                };
                memory.turns.push(user);
                memory.turns.push(TuiConversationTurn {
                    role: TuiConversationRole::Assistant,
                    content: record.content,
                });
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
) -> String {
    format!(
        "conversation-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\n{}",
            owner.project_id,
            owner.session_id,
            head_record_id.unwrap_or("root"),
            user_request
        ))[..24]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_memory_restores_only_complete_pairs_and_honors_reset_boundaries() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-tui-session-memory-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        crate::app::workflow_adapter::state::initialize().unwrap();

        let empty = load().unwrap();
        assert!(empty.turns.is_empty());

        let first = record_exchange(&empty, "내 이름은 감자야", "알겠습니다.").unwrap();
        assert_eq!(first.turns.len(), 2);
        assert_eq!(load().unwrap(), first);

        clear().unwrap();
        assert!(load().unwrap().turns.is_empty());

        let identity = ledger::validated_current_identity().unwrap();
        let owner = transcript_owner(&identity);
        transcript::record_session_turn(
            &owner,
            "user",
            "conversation-orphan-user",
            "응답 없는 요청",
            &[],
        )
        .unwrap();
        assert!(
            load().unwrap().turns.is_empty(),
            "불완전한 user/model pair는 prompt history에 들어가면 안 됩니다."
        );

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = std::fs::remove_dir_all(root);
    }
}
