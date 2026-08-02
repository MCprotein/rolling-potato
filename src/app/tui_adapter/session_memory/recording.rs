//! Append-only persistence for conversation results and typed tool activity.

use crate::app::web_search_adapter::WebGroundingEvidence;
use crate::app::workflow_adapter::{ledger, state, transcript};
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

use super::event_codec::{
    render_reset_event, render_runtime_error_event, render_tool_activity_event,
    render_web_grounding_event,
};
use super::restoration::{push_tool_activity, push_web_grounding};
use super::{transcript_owner, ConversationMemory, ConversationToolActivity};

const MAX_RUNTIME_ERROR_CHARS: usize = 2_048;

pub(super) fn record_exchange(
    memory: &mut ConversationMemory,
    user_request: &str,
    assistant_response: &str,
    web_grounding: &[WebGroundingEvidence],
    tool_activities: &[ConversationToolActivity],
) -> Result<(), AppError> {
    record_result(
        memory,
        user_request,
        assistant_response,
        "model",
        TuiConversationRole::Assistant,
        web_grounding,
        tool_activities,
    )
}

pub(super) fn record_failure(
    memory: &mut ConversationMemory,
    user_request: &str,
    runtime_error: &str,
    tool_activities: &[ConversationToolActivity],
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
        tool_activities,
    )
}

pub(in crate::app::tui_adapter) fn record_tool_activities(
    memory: &mut ConversationMemory,
    tool_activities: &[ConversationToolActivity],
) -> Result<(), AppError> {
    if tool_activities.is_empty() {
        return Ok(());
    }
    let identity = ledger::validated_current_identity()?;
    ensure_session_binding(memory, &identity)?;
    let owner = transcript_owner(&identity);
    append_tool_activity_records(memory, &owner, "tool-only", tool_activities)
}

#[allow(clippy::too_many_arguments)]
fn record_result(
    memory: &mut ConversationMemory,
    user_request: &str,
    response: &str,
    response_kind: &str,
    response_role: TuiConversationRole,
    web_grounding: &[WebGroundingEvidence],
    tool_activities: &[ConversationToolActivity],
) -> Result<(), AppError> {
    let identity = ledger::validated_current_identity()?;
    ensure_session_binding(memory, &identity)?;
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
    memory.head_record_id = Some(result.record_id);
    append_tool_activity_records(memory, &owner, &exchange_id, tool_activities)?;
    for (index, evidence) in web_grounding.iter().enumerate() {
        let record = transcript::record_session_turn(
            &owner,
            "evidence",
            &format!("{exchange_id}-web-evidence-{index}"),
            &render_web_grounding_event(evidence),
            &[],
        )?;
        memory.head_record_id = Some(record.record_id);
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
    Ok(())
}

fn append_tool_activity_records(
    memory: &mut ConversationMemory,
    owner: &transcript::TranscriptOwner,
    record_prefix: &str,
    tool_activities: &[ConversationToolActivity],
) -> Result<(), AppError> {
    for (index, activity) in tool_activities.iter().enumerate() {
        let record = transcript::record_session_turn(
            owner,
            "evidence",
            &format!(
                "{record_prefix}-tool-activity-{index}-{}",
                activity.execution_id
            ),
            &render_tool_activity_event(activity),
            &[],
        )?;
        push_tool_activity(&mut memory.tool_activities, activity.clone());
        memory.head_record_id = Some(record.record_id);
    }
    Ok(())
}

pub(in crate::app::tui_adapter) fn clear(memory: &mut ConversationMemory) -> Result<(), AppError> {
    let identity = ledger::validated_current_identity()?;
    ensure_session_binding(memory, &identity)?;
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
    memory.tool_activities.clear();
    memory.head_record_id = Some(reset.record_id);
    Ok(())
}

fn ensure_session_binding(
    memory: &ConversationMemory,
    identity: &ledger::RuntimeIdentity,
) -> Result<(), AppError> {
    if memory.belongs_to(&identity.session_id) {
        Ok(())
    } else {
        Err(AppError::blocked(
            "conversation memory session binding이 현재 session과 일치하지 않습니다.",
        ))
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
