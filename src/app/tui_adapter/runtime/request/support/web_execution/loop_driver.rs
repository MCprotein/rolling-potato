use crate::app::tui_adapter::conversation::{
    decide_web_observation_with_cancel, RequestDecision, WebObservationDecisionContext,
};
use crate::app::tui_adapter::session_memory::{ConversationToolActivity, ConversationToolStatus};
use crate::app::tui_adapter::web_tools;
use crate::app::web_search_adapter::{
    finish_observation, WebPageSession, WebResearchSession, WebToolObservation, WebToolRoute,
};
use crate::foundation::error::AppError;
use crate::runtime_core::agent::{AgentToolCall, AgentToolName, BoundedAgentLoop};
use crate::surfaces::tui::runtime_bridge::TuiRequestProgress;

use super::super::super::RequestExecution;
use super::{activity, web_execution};

pub(super) fn execute(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    route: WebToolRoute,
    context: web_tools::WebTurnContext<'_>,
    tool_activities: &mut Vec<ConversationToolActivity>,
) -> Result<RequestExecution, AppError> {
    let mut agent_loop = BoundedAgentLoop::default();
    let mut next_route = route;
    let mut fallback_observation = None;

    loop {
        if let Err(error) = context.cancellation.check() {
            tool_activities.push(activity::tool_activity(
                crate::surfaces::tui::runtime_bridge::new_tui_intent_id(),
                &next_route,
                ConversationToolStatus::Cancelled,
                &[],
            ));
            return Err(error);
        }
        if agent_loop
            .admit_tool_call(agent_tool_call(&next_route))
            .is_err()
        {
            return finish_or_block(research, fallback_observation);
        }

        let observation =
            match activity::observe_route(research, pages, next_route, context, tool_activities) {
                Ok(observation) => observation,
                Err(error) if context.cancellation.is_cancelled() => return Err(error),
                Err(_) if fallback_observation.is_some() => {
                    return finish_or_block(research, fallback_observation);
                }
                Err(error) => return Err(error),
            };
        let Some(model_context) = observation.model_context() else {
            return finish(research, observation, None);
        };
        if agent_loop.begin_follow_up_model_turn().is_err() {
            return finish(research, observation, None);
        }

        context.progress.emit(TuiRequestProgress::Deciding);
        let allowed_open_urls = pages
            .sources()
            .into_iter()
            .map(|source| source.url)
            .collect::<Vec<_>>();
        let mut agent_tool_history = context.tool_history.to_vec();
        agent_tool_history.extend(tool_activities.iter().cloned());
        let decision = decide_web_observation_with_cancel(WebObservationDecisionContext {
            observation: model_context,
            user_request: context.request,
            history: context.history,
            tool_activities: &agent_tool_history,
            context_limit_tokens: context.context_limit_tokens,
            allowed_open_urls: &allowed_open_urls,
            has_current_page: pages.current().is_some(),
            cancellation: context.cancellation,
        });
        let decision = match decision {
            Ok(decision) => decision,
            Err(error) if context.cancellation.is_cancelled() => return Err(error),
            Err(_) => return finish(research, observation, None),
        };
        match decision {
            RequestDecision::Answer(answer) => {
                context.progress.emit(TuiRequestProgress::Answering);
                return finish(research, observation, Some(answer));
            }
            RequestDecision::WebTool(route) => {
                fallback_observation = Some(observation);
                next_route = route;
            }
            RequestDecision::BrowserTool(_) | RequestDecision::ContinueLocal => {
                return finish(research, observation, None);
            }
        }
    }
}

fn finish_or_block(
    research: &mut WebResearchSession,
    observation: Option<WebToolObservation>,
) -> Result<RequestExecution, AppError> {
    let observation = observation
        .ok_or_else(|| AppError::blocked("agent tool loop가 안전한 실행 상한에 도달했습니다."))?;
    finish(research, observation, None)
}

fn finish(
    research: &mut WebResearchSession,
    observation: WebToolObservation,
    generated: Option<String>,
) -> Result<RequestExecution, AppError> {
    research.complete();
    Ok(web_execution(finish_observation(observation, generated)))
}

fn agent_tool_call(route: &WebToolRoute) -> AgentToolCall {
    let name = match route {
        WebToolRoute::Search { .. } => AgentToolName::Search,
        WebToolRoute::Open { .. } => AgentToolName::Open,
        WebToolRoute::Find { .. } => AgentToolName::Find,
    };
    AgentToolCall {
        name,
        input: route.input().to_string(),
    }
}
