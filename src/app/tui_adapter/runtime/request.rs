//! Interactive request routing for the canonical TUI conversation.

use super::super::{attachment, conversation, TuiRuntimeAdapter};
use super::backend::{ensure_runtime_ready, vision_status, RuntimeRequirement};
use crate::app::web_search_adapter::{self, WebToolRoute};
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiAttachment, TuiConversationTurn};
use std::time::Instant;

mod support;

use support::{
    execute_web_turn, plain_execution, required_context_limit, web_conversation_context,
};

pub(super) struct RequestExecution {
    pub(super) response: String,
    pub(super) web_grounding: Vec<crate::app::web_search_adapter::WebGroundingEvidence>,
}

pub(super) fn execute(
    adapter: &mut TuiRuntimeAdapter,
    request: &str,
    attachments: &[TuiAttachment],
    history: &[TuiConversationTurn],
    web_grounding: &[crate::app::web_search_adapter::WebGroundingEvidence],
) -> Result<RequestExecution, AppError> {
    let mut execution = execute_routed(adapter, request, attachments, history, web_grounding)?;
    execution.response = conversation::ensure_public_answer(execution.response)?;
    Ok(execution)
}

fn execute_routed(
    adapter: &mut TuiRuntimeAdapter,
    request: &str,
    attachments: &[TuiAttachment],
    history: &[TuiConversationTurn],
    web_grounding: &[crate::app::web_search_adapter::WebGroundingEvidence],
) -> Result<RequestExecution, AppError> {
    let web_started = Instant::now();
    let mut web_research = crate::app::web_search_adapter::WebResearchSession::default();
    let user_request = request.trim();
    let backend = crate::app::inference_adapter::backend::runtime_snapshot().ok();
    let context_limit_tokens = crate::app::inference_adapter::model::configured_context_length()
        .ok()
        .or_else(|| {
            backend
                .as_ref()
                .and_then(|snapshot| snapshot.context_limit_tokens)
        });
    let configured_model = crate::app::inference_adapter::model::configured_model_id();
    let active_model = configured_model.clone().or_else(|| {
        backend
            .as_ref()
            .and_then(|snapshot| snapshot.model_id.clone())
    });
    let vision = vision_status(backend.as_ref());
    let input = attachment::compose_request(request, attachments, context_limit_tokens)?;
    let local_context = input.text.as_str();
    if !input.images.is_empty() {
        ensure_runtime_ready(RuntimeRequirement::Vision)?;
        return conversation::reply_with_images(
            &input,
            history,
            required_context_limit(context_limit_tokens)?,
        )
        .map(plain_execution);
    }
    if let Some(route) = web_search_adapter::route_tool_request(user_request) {
        let web_conversation_context = match &route {
            WebToolRoute::Search { .. } => {
                web_conversation_context(history, user_request, context_limit_tokens)?
            }
            _ => String::new(),
        };
        return execute_web_turn(
            &mut web_research,
            &mut adapter.web_pages,
            route,
            user_request,
            local_context,
            &web_conversation_context,
            web_started.elapsed(),
        );
    }
    if let Some(reply) = conversation::local_reply(user_request, active_model.as_deref(), vision) {
        return Ok(plain_execution(reply));
    }
    ensure_runtime_ready(RuntimeRequirement::Text)?;
    let conversational = conversation::is_conversational_request(user_request);
    let has_text_attachments = !attachments.is_empty();
    if conversational
        && !has_text_attachments
        && web_search_adapter::can_reuse_prior_grounding(user_request, web_grounding)
    {
        let conversation_context =
            web_conversation_context(history, user_request, context_limit_tokens)?;
        return web_search_adapter::answer_from_grounding(
            user_request,
            &conversation_context,
            web_grounding,
        )
        .map(plain_execution);
    }
    match conversation::decide_request(
        user_request,
        history,
        required_context_limit(context_limit_tokens)?,
        conversational && !has_text_attachments,
    )? {
        conversation::RequestDecision::Answer(answer) => return Ok(plain_execution(answer)),
        conversation::RequestDecision::BrowserTool(tool) => {
            return crate::app::browser_adapter::search_form(tool).map(plain_execution);
        }
        conversation::RequestDecision::WebTool(tool) => {
            let web_conversation_context =
                web_conversation_context(history, user_request, context_limit_tokens)?;
            return execute_web_turn(
                &mut web_research,
                &mut adapter.web_pages,
                tool,
                user_request,
                local_context,
                &web_conversation_context,
                web_started.elapsed(),
            );
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
        .map(plain_execution);
    }
    crate::app::runtime_adapter::agent_run_report(local_context)
        .map(|report| conversation::present_agent_report(&report))
        .map(plain_execution)
}
