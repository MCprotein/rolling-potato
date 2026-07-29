use super::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_executes_structured_web_turn_and_returns_grounded_answer() {
    let fixture = NativeTerminalFixture::new("structured-web-conversation");
    let backend = fixture.start_conversation_backend();

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    let mark = terminal.mark();
    submit_visible_command(&mut terminal, "Rust 최신 릴리스를 검색해서 알려줘");
    let turn = terminal.wait_for_after(mark, "열린 원문을 바탕으로 생성한 최종 답변입니다.");
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
        "에이전트 한 턴은 도구 결정과 근거 답변 생성 두 번의 모델 요청이어야 합니다: {requests:#?}"
    );
    assert!(requests[0].contains("\"response_format\""));
    assert!(requests[0].contains("\"web_search\""));
    assert!(requests[0].contains(r#""answer":{"type":"string"}"#));
    assert!(!requests[0].contains(r#""answer":{"type":"string","maxLength":"#));
    assert!(!requests[1].contains("\"response_format\""));
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
    let turn = terminal.wait_for_after(mark, "열린 페이지를 근거로 생성한 최종 답변입니다.");
    assert!(!turn.contains(r#"{"decision":"web_open""#));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(requests.len(), 2, "{requests:#?}");
    assert!(requests[0].contains("\"response_format\""));
    assert!(requests[0].contains("\"web_open\""));
    assert!(!requests[1].contains("\"response_format\""));
    assert!(requests[1].contains("WEB_OPEN_CONTENT"));
    assert!(requests[1].contains("Rust 설치"));
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_executes_structured_web_find_before_answering() {
    let fixture = NativeTerminalFixture::new("structured-web-find");
    let backend = fixture.start_conversation_backend_with_responses(
        r#"{"decision":"web_find","input":"Rust","answer":""}"#,
        "페이지 내부 관찰을 근거로 생성한 최종 답변입니다. [source-f6c1fc4a4a917c01]",
    );

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    submit_visible_command(&mut terminal, "/open https://blog.example.net/rust-release");
    terminal.wait_for("페이지 내부 관찰을 근거로 생성한 최종 답변입니다.");
    backend.clear_request_bodies();

    let mark = terminal.mark();
    submit_visible_command(&mut terminal, "이 페이지에서 Rust 찾아줘");
    let turn = terminal.wait_for_after(mark, "페이지 내부 관찰을 근거로 생성한 최종 답변입니다.");
    assert!(!turn.contains(r#"{"decision":"web_find""#));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(requests.len(), 2, "{requests:#?}");
    assert!(requests[0].contains("\"response_format\""));
    assert!(requests[0].contains("\"web_find\""));
    assert!(!requests[1].contains("\"response_format\""));
    assert!(requests[1].contains("WEB_FIND_EVIDENCE"));
    assert!(requests[1].contains("Rust 설치"));
}
