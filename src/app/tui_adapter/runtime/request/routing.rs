use std::time::Instant;

use crate::app::tui_adapter::session_memory::ConversationToolActivity;
use crate::app::tui_adapter::{attachment, conversation, web_tools, TuiRuntimeAdapter};
use crate::app::web_search_adapter;
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::TuiRequestProgress;

use super::support::{
    execute_local_turn, execute_web_turn, plain_execution, required_context_limit,
    web_conversation_context, LocalTurnContext,
};
use super::{RequestContext, RequestExecution};
use crate::app::tui_adapter::runtime::backend::{
    ensure_runtime_ready, vision_status, RuntimeRequirement,
};

pub(super) fn execute_routed(
    adapter: &mut TuiRuntimeAdapter,
    context: &RequestContext<'_>,
    tool_activities: &mut Vec<ConversationToolActivity>,
) -> Result<RequestExecution, AppError> {
    let RequestContext {
        request,
        attachments,
        history,
        tool_history,
        web_grounding,
        progress,
        cancellation,
    } = context;
    cancellation.check()?;
    let web_started = Instant::now();
    let mut web_research = web_search_adapter::WebResearchSession::default();
    let user_request = request.trim();
    let backend = crate::app::inference_adapter::backend::runtime_snapshot().ok();
    let context_limit_tokens = crate::app::inference_adapter::model::configured_context_length()
        .ok()
        .or_else(|| {
            backend
                .as_ref()
                .and_then(|snapshot| snapshot.context_limit_tokens)
        });
    let active_model = crate::app::inference_adapter::model::configured_model_id().or_else(|| {
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
            tool_history,
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
        return execute_web_turn(
            &mut web_research,
            &mut adapter.web_pages,
            route,
            web_turn_context(context, user_request, context_limit_tokens, web_started),
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
            web_conversation_context(history, tool_history, user_request, context_limit_tokens)?;
        return web_search_adapter::answer_from_grounding_with_cancel(
            user_request,
            &conversation_context,
            web_grounding,
            cancellation,
        )
        .map(plain_execution);
    }
    progress.emit(TuiRequestProgress::Deciding);
    match conversation::decide_request_with_cancel(
        user_request,
        history,
        tool_history,
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
            return execute_web_turn(
                &mut web_research,
                &mut adapter.web_pages,
                tool,
                web_turn_context(
                    context,
                    user_request,
                    Some(required_context_limit(context_limit_tokens)?),
                    web_started,
                ),
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
            tool_history,
            required_context_limit(context_limit_tokens)?,
            cancellation,
        )
        .map(plain_execution);
    }
    progress.emit(TuiRequestProgress::LocalWork);
    cancellation.check()?;
    execute_local_turn(
        LocalTurnContext {
            request: user_request,
            local_context,
            history,
            tool_history,
            context_limit_tokens: required_context_limit(context_limit_tokens)?,
            progress,
            cancellation,
        },
        tool_activities,
    )
}

fn web_turn_context<'a>(
    request_context: &'a RequestContext<'a>,
    request: &'a str,
    context_limit_tokens: Option<u32>,
    started: Instant,
) -> web_tools::WebTurnContext<'a> {
    web_tools::WebTurnContext {
        request,
        history: request_context.history,
        tool_history: request_context.tool_history,
        context_limit_tokens,
        started,
        progress: request_context.progress,
        cancellation: request_context.cancellation,
    }
}
