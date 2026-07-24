//! Interactive request routing and transcript ownership.

use super::super::{attachment, conversation, web_tools, TuiRuntimeAdapter};
use super::backend::ensure_runtime_ready;
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiAttachment, TuiConversationTurn};

pub(super) struct RequestExecution {
    pub(super) response: String,
    pub(super) transcript_owner: TranscriptOwner,
}

pub(super) enum TranscriptOwner {
    TuiConversation,
    Workflow,
}

pub(super) fn execute(
    adapter: &mut TuiRuntimeAdapter,
    request: &str,
    attachments: &[TuiAttachment],
    history: &[TuiConversationTurn],
) -> Result<RequestExecution, AppError> {
    let user_request = request.trim();
    let backend = crate::app::inference_adapter::backend::runtime_snapshot().ok();
    let context_limit_tokens = crate::app::inference_adapter::model::configured_context_length()
        .ok()
        .or_else(|| {
            backend
                .as_ref()
                .and_then(|snapshot| snapshot.context_limit_tokens)
        });
    let active_model = backend
        .and_then(|snapshot| snapshot.model_id)
        .or_else(crate::app::inference_adapter::model::configured_model_id);
    let input = attachment::compose_request(request, attachments, context_limit_tokens)?;
    let local_context = input.text.as_str();
    if !input.images.is_empty() {
        ensure_runtime_ready()?;
        return conversation::reply_with_images(
            &input,
            history,
            required_context_limit(context_limit_tokens)?,
        )
        .map(tui_execution);
    }
    if let Some(result) =
        web_tools::dispatch(&mut adapter.opened_web_page, user_request, local_context)
    {
        return result.map(tui_execution);
    }
    if let Some(reply) = conversation::local_reply(user_request, active_model.as_deref()) {
        return Ok(tui_execution(reply));
    }
    ensure_runtime_ready()?;
    let conversational = conversation::is_conversational_request(user_request);
    let has_text_attachments = !attachments.is_empty();
    match conversation::decide_request(
        user_request,
        history,
        required_context_limit(context_limit_tokens)?,
        conversational && !has_text_attachments,
    )? {
        conversation::RequestDecision::Answer(answer) => return Ok(tui_execution(answer)),
        conversation::RequestDecision::WebTool(tool) => {
            return web_tools::execute(
                &mut adapter.opened_web_page,
                tool,
                user_request,
                local_context,
            )
            .map(tui_execution);
        }
        conversation::RequestDecision::ContinueLocal => {}
    }
    if conversational {
        return conversation::reply_with_context(
            user_request,
            local_context,
            history,
            required_context_limit(context_limit_tokens)?,
        )
        .map(tui_execution);
    }
    crate::app::runtime_adapter::agent_run_report(local_context).map(|report| RequestExecution {
        response: conversation::present_agent_report(&report),
        transcript_owner: TranscriptOwner::Workflow,
    })
}

fn tui_execution(response: String) -> RequestExecution {
    RequestExecution {
        response,
        transcript_owner: TranscriptOwner::TuiConversation,
    }
}

fn required_context_limit(context_limit_tokens: Option<u32>) -> Result<u32, AppError> {
    context_limit_tokens.filter(|value| *value > 0).ok_or_else(|| {
        AppError::blocked(
            "선택한 모델의 context length를 확인하지 못했습니다. /model에서 모델을 다시 선택하거나 /doctor로 backend 상태를 확인하세요.",
        )
    })
}
