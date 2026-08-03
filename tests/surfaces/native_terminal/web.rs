use super::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_automatically_searches_and_returns_grounded_answer() {
    let fixture = NativeTerminalFixture::new("structured-web-conversation");
    let backend = fixture.start_conversation_backend_with_structured_sequence(&[
        r#"{"decision":"web_search","input":"Rust 최신 릴리스","answer":""}"#,
        r#"{"decision":"web_open","input":"https://blog.example.net/rust-release","answer":""}"#,
        r#"{"decision":"web_find","input":"checksum","answer":""}"#,
        r#"{"decision":"answer","input":"","answer":"원문은 명령 실행 전에 출처와 checksum을 확인합니다. [source-f6c1fc4a4a917c01]"}"#,
    ]);

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
    assert_eq!(requests.len(), 4, "structured web requests: {requests:#?}");
    assert!(requests[0].contains("\"response_format\""));
    assert!(requests[0].contains(r#""answer":{"type":"string"}"#));
    assert!(!requests[0].contains(r#""answer":{"type":"string","maxLength":"#));
    assert!(requests[0].contains("\"web_search\""));
    assert!(requests[1].contains("WEB_SEARCH_RESULTS"));
    assert!(requests[1].contains("https://blog.example.net/rust-release"));
    assert!(!requests[1].contains("WEB_OPEN_CONTENT"));
    assert!(requests[2].contains("WEB_OPEN_CONTENT"));
    assert!(requests[2].contains("Rust 설치"));
    assert!(!requests[2].contains("WEB_FIND_EVIDENCE"));
    assert!(requests[3].contains("WEB_FIND_EVIDENCE"));
    assert!(requests[3].contains("checksum"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_executes_structured_web_open_before_answering() {
    let fixture = NativeTerminalFixture::new("structured-web-open");
    let backend = fixture.start_conversation_backend_with_structured_sequence(&[
        r#"{"decision":"web_open","input":"https://blog.example.net/rust-release","answer":""}"#,
        r#"{"decision":"answer","input":"","answer":"열린 페이지는 Rust 설치와 checksum 확인을 안내합니다. [source-f6c1fc4a4a917c01]"}"#,
    ]);

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
    let backend = fixture.start_conversation_backend_with_structured_sequence(&[
        r#"{"status":"supported","answer":"페이지를 열었습니다. [source-f6c1fc4a4a917c01]","source_ids":["source-f6c1fc4a4a917c01"]}"#,
        r#"{"decision":"web_find","input":"Rust","answer":""}"#,
        r#"{"decision":"answer","input":"","answer":"원문에는 checksum을 확인해야 한다고 나옵니다. [source-f6c1fc4a4a917c01]"}"#,
    ]);

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
    assert_eq!(requests.len(), 2, "web find requests: {requests:#?}");
    assert!(requests[0].contains("\"response_format\""));
    assert!(requests[1].contains("WEB_FIND_EVIDENCE"));
    assert!(requests[1].contains("Rust 설치"));
    assert!(!requests[1].contains("OPENED_DOCUMENTS"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_replans_from_tool_observations_before_answering() {
    let fixture = NativeTerminalFixture::new("structured-web-replanning");
    let backend = fixture.start_conversation_backend_with_structured_sequence(&[
        r#"{"decision":"web_open","input":"https://blog.example.net/rust-release","answer":""}"#,
        r#"{"decision":"web_find","input":"checksum","answer":""}"#,
        r#"{"decision":"answer","input":"","answer":"원문은 checksum 확인을 안내합니다. [source-f6c1fc4a4a917c01]"}"#,
    ]);

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    let mark = terminal.mark();
    submit_visible_command(
        &mut terminal,
        "https://blog.example.net/rust-release 열어서 설치 검증 방법을 알려줘",
    );
    let turn = terminal.wait_for_after(mark, "원문은 checksum 확인을 안내합니다.");
    assert!(turn.contains("[source-f6c1fc4a4a917c01]"));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(requests.len(), 3, "agent-loop requests: {requests:#?}");
    assert!(requests[1].contains("RUNTIME_WEB_OBSERVATION"));
    assert!(requests[1].contains("WEB_OPEN_CONTENT"));
    assert!(requests[1].contains("TOOL_ACTIVITY_MEMORY"));
    assert!(requests[1].contains(r#"\"tool\":\"web_open\""#));
    assert!(requests[2].contains("RUNTIME_WEB_OBSERVATION"));
    assert!(requests[2].contains("WEB_FIND_EVIDENCE"));
}
