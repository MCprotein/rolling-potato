use crate::app::tui_adapter::session_memory::{
    ConversationToolActivity, ConversationToolName, ConversationToolStatus,
};
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

fn web_execution(execution: web_tools::WebToolExecution) -> RequestExecution {
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
    let progress = context.progress;
    let request = context.request;
    let cancellation = context.cancellation;
    let activity_route = route.clone();
    let execution_id = crate::surfaces::tui::runtime_bridge::new_tui_intent_id();
    let observation = match web_tools::observe(research, pages, route, context) {
        Ok(observation) => observation,
        Err(error) => {
            tool_activities.push(tool_activity(
                execution_id,
                &activity_route,
                if cancellation.is_cancelled() {
                    ConversationToolStatus::Cancelled
                } else if error.code == 3 {
                    ConversationToolStatus::Blocked
                } else {
                    ConversationToolStatus::Failed
                },
                &[],
            ));
            return Err(error);
        }
    };
    progress.emit(crate::surfaces::tui::runtime_bridge::TuiRequestProgress::Answering);
    let execution = match web_tools::answer(observation, request, cancellation) {
        Ok(execution) => execution,
        Err(error) => {
            tool_activities.push(tool_activity(
                execution_id,
                &activity_route,
                if cancellation.is_cancelled() {
                    ConversationToolStatus::Cancelled
                } else {
                    ConversationToolStatus::Failed
                },
                &[],
            ));
            return Err(error);
        }
    };
    let source_ids = execution
        .grounding
        .iter()
        .map(|evidence| evidence.source_id.clone())
        .collect::<Vec<_>>();
    tool_activities.push(tool_activity(
        execution_id,
        &activity_route,
        ConversationToolStatus::Succeeded,
        &source_ids,
    ));
    Ok(web_execution(execution))
}

fn tool_activity(
    execution_id: String,
    route: &WebToolRoute,
    status: ConversationToolStatus,
    source_ids: &[String],
) -> ConversationToolActivity {
    let tool = match route {
        WebToolRoute::Search { .. } => ConversationToolName::Search,
        WebToolRoute::Open { .. } => ConversationToolName::Open,
        WebToolRoute::Find { .. } => ConversationToolName::Find,
    };
    ConversationToolActivity::bounded(
        execution_id,
        tool,
        route.input(),
        status,
        source_ids.iter().cloned(),
    )
}
