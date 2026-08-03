use super::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn malformed_follow_up_finishes_with_the_last_verified_observation() {
    let fixture = NativeTerminalFixture::new("malformed-web-follow-up");
    let backend = fixture.start_conversation_backend_with_structured_sequence(&[
        r#"{"decision":"web_open","input":"https://blog.example.net/rust-release","answer":""}"#,
        "not a structured decision",
    ]);

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    let mark = terminal.mark();
    submit_visible_command(
        &mut terminal,
        "https://blog.example.net/rust-release 열어서 요약해줘",
    );
    let turn = terminal.wait_for_after(mark, "근거 · [source-f6c1fc4a4a917c01] 안전한 Rust 안내서");
    assert!(turn.contains("페이지를 열었습니다."));
    assert!(turn.contains("checksum을 확인합니다."));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(
        requests.len(),
        2,
        "malformed follow-up requests: {requests:#?}"
    );
    assert!(requests[1].contains("WEB_OPEN_CONTENT"));
    assert!(requests[1].contains("TOOL_ACTIVITY_MEMORY"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn non_consecutive_repeated_tool_call_stops_before_another_network_step() {
    let fixture = NativeTerminalFixture::new("non-consecutive-repeated-web-tool");
    let backend = fixture.start_conversation_backend_with_structured_sequence(&[
        r#"{"decision":"web_search","input":"Rust 최신 릴리스","answer":""}"#,
        r#"{"decision":"web_open","input":"https://blog.example.net/rust-release","answer":""}"#,
        r#"{"decision":"web_search","input":"Rust 최신 릴리스","answer":""}"#,
    ]);

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    let mark = terminal.mark();
    submit_visible_command(
        &mut terminal,
        "Rust 최신 릴리스를 검색해서 원문으로 확인해줘",
    );
    let turn = terminal.wait_for_after(mark, "근거 · [source-f6c1fc4a4a917c01] 안전한 Rust 안내서");
    assert!(turn.contains("페이지를 열었습니다."));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(requests.len(), 3, "repeated tool requests: {requests:#?}");
    assert!(requests[1].contains("WEB_SEARCH_RESULTS"));
    assert!(requests[2].contains("WEB_OPEN_CONTENT"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn resumed_conversation_supplies_prior_typed_tool_activity_without_replaying_it() {
    let fixture = NativeTerminalFixture::new("resumed-tool-activity-memory");
    let _live_terminal = LiveTerminalEnvironment::enable();
    let backend = fixture.start_conversation_backend_with_structured_sequence(&[
        r#"{"decision":"web_search","input":"Rust 최신 릴리스","answer":""}"#,
        r#"{"decision":"answer","input":"","answer":"검색 결과를 확인했습니다. [source-f6c1fc4a4a917c01]"}"#,
        r#"{"decision":"answer","input":"","answer":"이전 검색은 성공했습니다."}"#,
    ]);

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    submit_visible_command(&mut terminal, "Rust 최신 릴리스를 검색해서 알려줘");
    terminal.wait_for("근거 · [source-f6c1fc4a4a917c01] Third-party Rust release summary");
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    backend.clear_request_bodies();
    let mut resumed = NativePty::spawn(120, 40);
    resumed.wait_for("session new");
    submit_visible_command(&mut resumed, "/resume");
    resumed.wait_for("세션 재개");
    resumed.send("1");
    resumed.wait_for("선택한 세션을 재개했습니다.");
    let mark = resumed.mark();
    submit_visible_command(&mut resumed, "이 세션의 작업 기록을 한 문장으로 정리해줘");
    resumed.wait_for_after(mark, "이전 검색은 성공했습니다.");
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
