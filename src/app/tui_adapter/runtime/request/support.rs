use super::super::super::{conversation, web_tools};
use super::RequestExecution;
use crate::app::web_search_adapter::{WebPageSession, WebResearchSession, WebToolRoute};
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::TuiConversationTurn;
use std::time::Duration;

pub(super) fn plain_execution(response: String) -> RequestExecution {
    RequestExecution {
        response,
        web_grounding: Vec::new(),
    }
}

pub(super) fn web_execution(execution: web_tools::WebToolExecution) -> RequestExecution {
    RequestExecution {
        response: execution.response,
        web_grounding: execution.grounding,
    }
}

pub(super) fn execute_web_turn(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    route: WebToolRoute,
    request: &str,
    local_context: &str,
    conversation_context: &str,
    elapsed: Duration,
) -> Result<RequestExecution, AppError> {
    let observation = web_tools::observe(
        research,
        pages,
        route,
        request,
        local_context,
        conversation_context,
        elapsed,
    )?;
    Ok(web_execution(web_tools::answer(observation, request)))
}

pub(super) fn web_conversation_context(
    history: &[TuiConversationTurn],
    user_request: &str,
    context_limit_tokens: Option<u32>,
) -> Result<String, AppError> {
    if history.is_empty() {
        return Ok(String::new());
    }
    conversation::render_web_conversation_context(
        history,
        user_request,
        required_context_limit(context_limit_tokens)?,
    )
}

pub(super) fn required_context_limit(context_limit_tokens: Option<u32>) -> Result<u32, AppError> {
    context_limit_tokens.filter(|value| *value > 0).ok_or_else(|| {
        AppError::blocked(
            "선택한 모델의 context length를 확인하지 못했습니다. /model에서 모델을 다시 선택하거나 /doctor로 backend 상태를 확인하세요.",
        )
    })
}
