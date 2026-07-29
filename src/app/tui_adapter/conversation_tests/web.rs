use super::*;

#[test]
fn search_command_routes_the_question_and_renders_the_answer() {
    let mut terminal = ScriptedTerminal::new(["/search Rust 공식 웹사이트는?", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert_eq!(runtime.requests, ["/search Rust 공식 웹사이트는?"]);
    assert!(rendered.contains("› /search Rust 공식 웹사이트는?"));
    assert!(rendered.contains("웹 조사 · 검색 중"));
    assert!(rendered.contains("검색 ● → 결과 평가 ○ → 문서 읽기 ○ → 증거 구성 ○ → 답변 ○"));
    assert!(rendered.contains("● 안녕하세요."));
}

#[test]
fn web_open_and_find_commands_route_through_the_conversation_runtime() {
    let mut terminal =
        ScriptedTerminal::new(["/open https://example.com/docs", "/find ownership", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(
        runtime.requests,
        ["/open https://example.com/docs", "/find ownership"]
    );
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("페이지 여는 중"));
    assert!(rendered.contains("페이지 찾는 중"));
}

#[test]
fn sources_command_uses_a_picker_and_changes_the_current_document() {
    let mut terminal = ScriptedTerminal::new(["/sources", "2", "/quit"]);
    let mut runtime = ConversationRuntime {
        web_source_options: vec![
            web_source_option("source-one", "첫 문서", "https://example.com/one", true),
            web_source_option("source-two", "둘째 문서", "https://example.com/two", false),
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.selected_web_sources, ["source-two"]);
    assert!(terminal
        .frames
        .join("\n")
        .contains("현재 웹 출처를 변경했습니다: source-two"));
}

#[test]
fn interactive_web_open_keeps_page_available_for_followup_find() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-interactive-web-tools-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_TEST_SKIP_UPDATE_CHECK", "1");
    std::env::set_var(
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "<html><title>Rust Guide</title><body>Ownership is a Rust feature.</body></html>",
    );
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new([
        "/open https://example.com/guide",
        "/find ownership",
        "/quit",
    ]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter::default()).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_TEST_SKIP_UPDATE_CHECK");
    std::env::remove_var("RPOTATO_TEST_WEB_OPEN_HTML");
    let _ = std::fs::remove_dir_all(root);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("Rust Guide"));
    assert!(rendered.contains("일치: 1개"));
    assert!(rendered.contains("Ownership is a Rust feature."));
}

#[test]
fn search_results_enter_the_source_picker_and_open_for_followup_find() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-search-source-picker-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_TEST_SKIP_UPDATE_CHECK", "1");
    std::env::set_var(
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        include_str!("../../../../tests/fixtures/web_search/ddg-html.html"),
    );
    std::env::set_var(
        "RPOTATO_TEST_WEB_OPEN_HTML",
        "<html><title>Selected Source</title><main>Verified checksum evidence.</main></html>",
    );
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new([
        "/search Rust official release",
        "/sources",
        "1",
        "/find checksum",
        "/quit",
    ]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter::default()).unwrap();

    for name in [
        "RPOTATO_PROJECT_ROOT",
        "RPOTATO_DATA_HOME",
        "RPOTATO_TEST_SKIP_UPDATE_CHECK",
        "RPOTATO_TEST_WEB_SEARCH_HTML",
        "RPOTATO_TEST_WEB_OPEN_HTML",
    ] {
        std::env::remove_var(name);
    }
    let _ = std::fs::remove_dir_all(root);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("Selected Source"));
    assert!(rendered.contains("일치: 1개"));
    assert!(rendered.contains("Verified checksum evidence."));
}
