use super::super::*;
use crate::app::tui_adapter::session_memory::{ConversationToolName, ConversationToolStatus};
use crate::app::tui_adapter::web_tools;
use crate::app::web_search_adapter::{WebPageSession, WebResearchSession, WebToolRoute};
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
        history: &[],
        tool_history: &[],
        context_limit_tokens: 4_096,
        started: std::time::Instant::now(),
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
        history: &[],
        tool_history: &[],
        context_limit_tokens: 4_096,
        started: std::time::Instant::now(),
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

    let cancelled_token = RequestCancellationToken::default();
    cancelled_token.cancel();
    let mut cancelled = Vec::new();
    let error = match execute_web_turn(
        &mut WebResearchSession::default(),
        &mut WebPageSession::default(),
        WebToolRoute::Search {
            query: "Rust stable release".to_string(),
        },
        web_tools::WebTurnContext {
            request: "Rust stable release를 검색해줘",
            history: &[],
            tool_history: &[],
            context_limit_tokens: 4_096,
            started: std::time::Instant::now(),
            progress: &progress,
            cancellation: &cancelled_token,
        },
        &mut cancelled,
    ) {
        Ok(_) => panic!("cancelled route must not execute"),
        Err(error) => error,
    };
    assert_eq!(error.message, "요청을 취소했습니다.");
    assert_eq!(cancelled.len(), 1);
    assert_eq!(cancelled[0].tool, ConversationToolName::Search);
    assert_eq!(cancelled[0].status, ConversationToolStatus::Cancelled);

    let mut elapsed = Vec::new();
    let expired = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(45))
        .expect("45 seconds before now");
    let error = match execute_web_turn(
        &mut WebResearchSession::default(),
        &mut WebPageSession::default(),
        WebToolRoute::Search {
            query: "Rust stable release".to_string(),
        },
        web_tools::WebTurnContext {
            request: "Rust stable release를 검색해줘",
            history: &[],
            tool_history: &[],
            context_limit_tokens: 4_096,
            started: expired,
            progress: &progress,
            cancellation: &cancellation,
        },
        &mut elapsed,
    ) {
        Ok(_) => panic!("expired research budget must not execute"),
        Err(error) => error,
    };
    assert_eq!(error.message, "웹 리서치 시간 상한에 도달했습니다.");
    assert_eq!(elapsed.len(), 1);
    assert_eq!(elapsed[0].tool, ConversationToolName::Search);
    assert_eq!(elapsed[0].status, ConversationToolStatus::Blocked);
    std::env::remove_var("RPOTATO_TEST_WEB_RESEARCH_NO_MODEL");
}
