use super::super::super::session_memory::{
    ConversationToolActivity, ConversationToolName, ConversationToolStatus,
};
use super::super::super::{conversation, web_tools};
use super::RequestExecution;
use crate::app::web_search_adapter::{WebPageSession, WebResearchSession, WebToolRoute};
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::TuiConversationTurn;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::inference::cancellation::RequestCancellationToken;
    use crate::surfaces::tui::runtime_bridge::TuiRequestProgressReporter;

    #[test]
    fn web_turn_records_typed_success_and_blocked_activity() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        std::env::set_var("RPOTATO_TEST_WEB_RESEARCH_NO_MODEL", "1");
        let cancellation = RequestCancellationToken::default();
        let progress = TuiRequestProgressReporter::default();
        let mut research = WebResearchSession::default();
        let mut pages = WebPageSession::default();
        pages.record(crate::adapters::web_search::WebPageEvidence {
            source_id: "source-doc".to_string(),
            requested_url: "https://example.com/doc".to_string(),
            final_url: "https://example.com/doc".to_string(),
            title: Some("Example".to_string()),
            content: "Rust stable release notes".to_string(),
        });
        let mut activities = Vec::new();
        let context = web_tools::WebTurnContext {
            request: "stable을 찾아줘",
            local_context: "",
            conversation_context: "",
            elapsed: std::time::Duration::ZERO,
            progress: &progress,
            cancellation: &cancellation,
        };

        let execution = execute_web_turn(
            &mut research,
            &mut pages,
            WebToolRoute::Find {
                query: "stable".to_string(),
            },
            context,
            &mut activities,
        )
        .unwrap();

        assert!(execution.response.contains("stable"));
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].tool, ConversationToolName::Find);
        assert_eq!(activities[0].status, ConversationToolStatus::Succeeded);
        assert_eq!(activities[0].source_ids, ["source-doc"]);

        let mut blocked = Vec::new();
        let context = web_tools::WebTurnContext {
            request: "잘못된 검색",
            local_context: "",
            conversation_context: "",
            elapsed: std::time::Duration::ZERO,
            progress: &progress,
            cancellation: &cancellation,
        };
        let error = match execute_web_turn(
            &mut WebResearchSession::default(),
            &mut WebPageSession::default(),
            WebToolRoute::Search {
                query: "invalid\nquery".to_string(),
            },
            context,
            &mut blocked,
        ) {
            Ok(_) => panic!("invalid route must be blocked"),
            Err(error) => error,
        };
        assert_eq!(error.code, 3);
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].status, ConversationToolStatus::Blocked);
        std::env::remove_var("RPOTATO_TEST_WEB_RESEARCH_NO_MODEL");
    }
}
