use crate::app::tui_adapter::session_memory::{
    ConversationToolActivity, ConversationToolName, ConversationToolStatus,
};
use crate::app::tui_adapter::web_tools;
use crate::app::web_search_adapter::{
    WebPageSession, WebResearchSession, WebResearchTraceStatus, WebResearchTraceStep, WebToolRoute,
};
use crate::foundation::error::AppError;

pub(super) fn observe_route(
    research: &mut WebResearchSession,
    pages: &mut WebPageSession,
    route: WebToolRoute,
    context: web_tools::WebTurnContext<'_>,
    tool_activities: &mut Vec<ConversationToolActivity>,
) -> Result<crate::app::web_search_adapter::WebToolObservation, AppError> {
    let activity_route = route.clone();
    let activity_start = tool_activities.len();
    let mut trace = Vec::new();
    match web_tools::observe(research, pages, route, context, &mut trace) {
        Ok(observation) => {
            append_trace(tool_activities, trace);
            Ok(observation)
        }
        Err(error) => {
            append_trace(tool_activities, trace);
            if tool_activities.len() == activity_start {
                tool_activities.push(tool_activity(
                    crate::surfaces::tui::runtime_bridge::new_tui_intent_id(),
                    &activity_route,
                    if context.cancellation.is_cancelled() {
                        ConversationToolStatus::Cancelled
                    } else if error.code == 3 {
                        ConversationToolStatus::Blocked
                    } else {
                        ConversationToolStatus::Failed
                    },
                    &[],
                ));
            }
            Err(error)
        }
    }
}

fn append_trace(
    tool_activities: &mut Vec<ConversationToolActivity>,
    trace: Vec<WebResearchTraceStep>,
) {
    tool_activities.extend(trace.into_iter().map(|step| {
        let status = match step.status {
            WebResearchTraceStatus::Succeeded => ConversationToolStatus::Succeeded,
            WebResearchTraceStatus::Failed => ConversationToolStatus::Failed,
            WebResearchTraceStatus::Blocked => ConversationToolStatus::Blocked,
            WebResearchTraceStatus::Cancelled => ConversationToolStatus::Cancelled,
        };
        tool_activity(
            crate::surfaces::tui::runtime_bridge::new_tui_intent_id(),
            &step.route,
            status,
            &step.source_ids,
        )
    }));
}

pub(super) fn tool_activity(
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
