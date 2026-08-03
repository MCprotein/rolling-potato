mod activity;
mod loop_driver;

use crate::app::tui_adapter::session_memory::ConversationToolActivity;
use crate::app::tui_adapter::web_tools;
use crate::app::web_search_adapter::{WebPageSession, WebResearchSession, WebToolRoute};
use crate::foundation::error::AppError;

use super::super::RequestExecution;

pub(in crate::app::tui_adapter::runtime::request) fn plain_execution(
    response: String,
) -> RequestExecution {
    RequestExecution {
        response,
        web_grounding: Vec::new(),
    }
}

pub(super) fn web_execution(
    execution: crate::app::web_search_adapter::WebAnswerResult,
) -> RequestExecution {
    RequestExecution {
        response: execution.response,
        web_grounding: execution.grounding,
    }
}

pub(in crate::app::tui_adapter::runtime::request) fn execute_web_turn(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    route: WebToolRoute,
    context: web_tools::WebTurnContext<'_>,
    tool_activities: &mut Vec<ConversationToolActivity>,
) -> Result<RequestExecution, AppError> {
    loop_driver::execute(research, pages, route, context, tool_activities)
}
