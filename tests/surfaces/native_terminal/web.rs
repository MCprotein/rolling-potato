use super::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_automatically_searches_and_returns_grounded_answer() {
    let fixture = NativeTerminalFixture::new("structured-web-conversation");
    let backend = fixture.start_conversation_backend_with_responses(
        r#"{"decision":"web_search","input":"Rust 최신 릴리스","answer":""}"#,
        "열린 원문을 바탕으로 생성한 최종 답변입니다. [source-f6c1fc4a4a917c01]",
    );

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    let mark = terminal.mark();
    submit_visible_command(&mut terminal, "Rust 최신 릴리스를 검색해서 알려줘");
    let turn = terminal.wait_for_after(mark, "근거 · [source-f6c1fc4a4a917c01] 안전한 Rust 안내서");
    assert!(turn.contains("checksum을 확인합니다."));
    assert!(turn.contains("안전한 Rust 안내서"));
    assert!(turn.contains("[source-f6c1fc4a4a917c01]"));
    assert!(!turn.contains(r#"{"decision":"web_search""#));
    assert!(!turn.contains("WEB TOOL:"));
    assert!(!turn.contains("WEB INPUT:"));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(
        requests.len(),
        2,
        "최신 정보 요청은 모델의 구조화된 도구 결정과 근거 답변 생성이 모두 실행되어야 합니다: {requests:#?}"
    );
    assert!(requests[0].contains("\"response_format\""));
    assert!(requests[0].contains(r#""answer":{"type":"string"}"#));
    assert!(!requests[0].contains(r#""answer":{"type":"string","maxLength":"#));
    assert!(requests[0].contains("\"web_search\""));
    assert!(requests[1].contains("OPENED_DOCUMENTS"));
    assert!(requests[1].contains("안전한 Rust 안내서"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_executes_structured_web_open_before_answering() {
    let fixture = NativeTerminalFixture::new("structured-web-open");
    let backend = fixture.start_conversation_backend_with_responses(
        r#"{"decision":"web_open","input":"https://blog.example.net/rust-release","answer":""}"#,
        "열린 페이지를 근거로 생성한 최종 답변입니다. [source-f6c1fc4a4a917c01]",
    );

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    let mark = terminal.mark();
    submit_visible_command(
        &mut terminal,
        "https://blog.example.net/rust-release 열어서 요약해줘",
    );
    let turn = terminal.wait_for_after(mark, "근거 · [source-f6c1fc4a4a917c01] 안전한 Rust 안내서");
    assert!(turn.contains("안전한 Rust 안내서"));
    assert!(turn.contains("Rust 설치"));
    assert!(turn.contains("[source-f6c1fc4a4a917c01]"));
    assert!(!turn.contains(r#"{"decision":"web_open""#));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(requests.len(), 2, "{requests:#?}");
    assert!(requests[0].contains("\"response_format\""));
    assert!(requests[0].contains("\"web_open\""));
    assert!(requests[1].contains("\"response_format\""));
    assert!(requests[1].contains("WEB_OPEN_CONTENT"));
    assert!(requests[1].contains("Rust 설치"));
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_finds_within_the_current_page_before_answering() {
    let fixture = NativeTerminalFixture::new("structured-web-find");
    let backend = fixture.start_conversation_backend_with_responses(
        r#"{"decision":"web_find","input":"Rust","answer":""}"#,
        "페이지 내부 관찰을 근거로 생성한 최종 답변입니다. [source-f6c1fc4a4a917c01]",
    );

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    submit_visible_command(&mut terminal, "/open https://blog.example.net/rust-release");
    terminal.wait_for("근거 · [source-f6c1fc4a4a917c01] 안전한 Rust 안내서");
    backend.clear_request_bodies();

    let mark = terminal.mark();
    submit_visible_command(&mut terminal, "이 페이지에서 Rust 찾아줘");
    let turn = terminal.wait_for_after(mark, "근거 · [source-f6c1fc4a4a917c01] 안전한 Rust 안내서");
    assert!(turn.contains("checksum을 확인합니다."));
    assert!(turn.contains("안전한 Rust 안내서"));
    assert!(turn.contains("[source-f6c1fc4a4a917c01]"));
    assert!(!turn.contains(r#"{"decision":"web_find""#));
    assert!(!turn.contains("WEB TOOL:"));
    assert!(!turn.contains("WEB INPUT:"));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(
        requests.len(),
        1,
        "열린 페이지를 명시한 찾기 요청은 새 검색이나 별도 도구 결정 없이 현재 페이지 근거 답변만 생성해야 합니다: {requests:#?}"
    );
    assert!(requests[0].contains("\"response_format\""));
    assert!(requests[0].contains("WEB_FIND_EVIDENCE"));
    assert!(requests[0].contains("Rust 설치"));
    assert!(!requests[0].contains("OPENED_DOCUMENTS"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn resumed_conversation_supplies_prior_typed_tool_activity_without_replaying_it() {
    let fixture = NativeTerminalFixture::new("resumed-tool-activity-memory");
    let _live_terminal = LiveTerminalEnvironment::enable();
    let backend = fixture.start_conversation_backend_with_responses(
        r#"{"decision":"web_search","input":"Rust 최신 릴리스","answer":""}"#,
        "열린 원문을 바탕으로 생성한 최종 답변입니다. [source-f6c1fc4a4a917c01]",
    );

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    submit_visible_command(&mut terminal, "Rust 최신 릴리스를 검색해서 알려줘");
    terminal.wait_for("근거 · [source-f6c1fc4a4a917c01] 안전한 Rust 안내서");
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    backend.clear_request_bodies();
    backend.set_structured_response(
        r#"{"decision":"answer","input":"","answer":"이전 검색은 성공했습니다."}"#,
    );
    let mut resumed = NativePty::spawn(120, 40);
    resumed.wait_for("session new");
    submit_visible_command(&mut resumed, "/resume");
    resumed.wait_for("세션 재개");
    resumed.send("1");
    resumed.wait_for("선택한 세션을 재개했습니다.");
    submit_visible_command(&mut resumed, "이 세션의 작업 기록을 한 문장으로 정리해줘");
    resumed.wait_for("이전 검색은 성공했습니다.");
    submit_visible_command(&mut resumed, "/quit");
    resumed.finish();

    let requests = backend.request_bodies();
    assert_eq!(
        requests.len(),
        1,
        "resume follow-up must not replay web: {requests:#?}"
    );
    assert!(requests[0].contains("TOOL_ACTIVITY_MEMORY"));
    assert!(requests[0].contains(r#"\"tool\":\"web_search\""#));
    assert!(requests[0].contains(r#"\"status\":\"succeeded\""#));
    assert!(requests[0].contains("Rust 최신 릴리스"));
    assert!(requests[0].contains("source-f6c1fc4a4a917c01"));
}
