use super::super::*;
use crate::app::tui_adapter::session_memory::{ConversationToolName, ConversationToolStatus};
use crate::app::tui_adapter::web_tools;
use crate::app::web_search_adapter::{WebPageSession, WebResearchSession, WebToolRoute};
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::surfaces::tui::runtime_bridge::TuiRequestProgressReporter;

#[test]
fn search_records_one_typed_activity_without_implicit_open_or_find() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    std::env::set_var(
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        r#"<html><body><div class="result results_links web-result">
            <h2 class="result__title"><a class="result__a" href="https://example.com/rust">Rust release</a></h2>
            <a class="result__snippet">Rust stable release notes</a>
        </div></body></html>"#,
    );
    std::env::set_var(
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "<html><title>Rust release</title><main>Rust stable release notes</main></html>",
    );
    std::env::set_var("RPOTATO_TEST_WEB_RESEARCH_NO_MODEL", "1");
    let cancellation = RequestCancellationToken::default();
    let progress = TuiRequestProgressReporter::default();
    let mut activities = Vec::new();

    execute_web_turn(
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
            cancellation: &cancellation,
        },
        &mut activities,
    )
    .unwrap();

    for name in [
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "RPOTATO_TEST_WEB_RESEARCH_NO_MODEL",
    ] {
        std::env::remove_var(name);
    }
    assert_eq!(activities.len(), 1, "{activities:?}");
    assert_eq!(activities[0].tool, ConversationToolName::Search);
    assert_eq!(activities[0].status, ConversationToolStatus::Succeeded);
    assert_eq!(activities[0].source_ids.len(), 1);
}
