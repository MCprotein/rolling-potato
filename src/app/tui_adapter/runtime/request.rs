//! Interactive request routing for the canonical TUI conversation.

use super::super::session_memory::ConversationToolActivity;
use super::super::{attachment, conversation, TuiRuntimeAdapter};
use super::backend::{ensure_runtime_ready, vision_status, RuntimeRequirement};
use crate::app::web_search_adapter::{self, WebToolRoute};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::surfaces::tui::runtime_bridge::{
    TuiAttachment, TuiConversationTurn, TuiRequestProgress, TuiRequestProgressReporter,
};
use std::time::Instant;

mod support;

use support::{
    execute_web_turn, plain_execution, required_context_limit, web_conversation_context,
};

pub(super) struct RequestExecution {
    pub(super) response: String,
    pub(super) web_grounding: Vec<crate::app::web_search_adapter::WebGroundingEvidence>,
}

pub(super) struct RequestContext<'a> {
    pub(super) request: &'a str,
    pub(super) attachments: &'a [TuiAttachment],
    pub(super) history: &'a [TuiConversationTurn],
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
    let mut execution = execute_routed(adapter, &context, tool_activities)?;
    context.cancellation.check()?;
    execution.response = conversation::ensure_public_answer(execution.response)?;
    Ok(execution)
}

fn execute_routed(
    adapter: &mut TuiRuntimeAdapter,
    context: &RequestContext<'_>,
    tool_activities: &mut Vec<ConversationToolActivity>,
) -> Result<RequestExecution, AppError> {
    let RequestContext {
        request,
        attachments,
        history,
        web_grounding,
        progress,
        cancellation,
    } = context;
    cancellation.check()?;
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
        progress.emit(TuiRequestProgress::Answering);
        return conversation::reply_with_images_and_cancel(
            &input,
            history,
            required_context_limit(context_limit_tokens)?,
            cancellation,
        )
        .map(plain_execution);
    }
    let immediate_web_route = web_search_adapter::route_tool_request(user_request).or_else(|| {
        web_search_adapter::route_current_page_find(
            user_request,
            adapter.web_pages.current().is_some(),
        )
    });
    if let Some(route) = immediate_web_route {
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
            super::super::web_tools::WebTurnContext {
                request: user_request,
                local_context,
                conversation_context: &web_conversation_context,
                elapsed: web_started.elapsed(),
                progress,
                cancellation,
            },
            tool_activities,
        );
    }
    if let Some(reply) = conversation::local_reply(user_request, active_model.as_deref(), vision) {
        progress.emit(TuiRequestProgress::Answering);
        return Ok(plain_execution(reply));
    }
    ensure_runtime_ready(RuntimeRequirement::Text)?;
    let conversational = conversation::is_conversational_request(user_request);
    let has_text_attachments = !attachments.is_empty();
    if conversational
        && !has_text_attachments
        && web_search_adapter::can_reuse_prior_grounding(user_request, web_grounding)
    {
        progress.emit(TuiRequestProgress::Answering);
        let conversation_context =
            web_conversation_context(history, user_request, context_limit_tokens)?;
        return web_search_adapter::answer_from_grounding(
            user_request,
            &conversation_context,
            web_grounding,
        )
        .map(plain_execution);
    }
    progress.emit(TuiRequestProgress::Deciding);
    match conversation::decide_request_with_cancel(
        user_request,
        history,
        required_context_limit(context_limit_tokens)?,
        conversational && !has_text_attachments,
        cancellation,
    )? {
        conversation::RequestDecision::Answer(answer) => {
            progress.emit(TuiRequestProgress::Answering);
            return Ok(plain_execution(answer));
        }
        conversation::RequestDecision::BrowserTool(tool) => {
            progress.emit(TuiRequestProgress::LocalWork);
            return crate::app::browser_adapter::search_form(tool).map(plain_execution);
        }
        conversation::RequestDecision::WebTool(tool) => {
            let web_conversation_context =
                web_conversation_context(history, user_request, context_limit_tokens)?;
            return execute_web_turn(
                &mut web_research,
                &mut adapter.web_pages,
                tool,
                super::super::web_tools::WebTurnContext {
                    request: user_request,
                    local_context,
                    conversation_context: &web_conversation_context,
                    elapsed: web_started.elapsed(),
                    progress,
                    cancellation,
                },
                tool_activities,
            );
        }
        conversation::RequestDecision::ContinueLocal => {}
    }
    if conversational {
        progress.emit(TuiRequestProgress::Answering);
        return conversation::reply_with_context_and_cancel(
            user_request,
            local_context,
            history,
            required_context_limit(context_limit_tokens)?,
            cancellation,
        )
        .map(plain_execution);
    }
    progress.emit(TuiRequestProgress::LocalWork);
    cancellation.check()?;
    crate::app::runtime_adapter::agent_run_report(local_context)
        .map(|report| conversation::present_agent_report(&report))
        .map(plain_execution)
}
