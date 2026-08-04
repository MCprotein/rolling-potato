use super::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn conversation_agent_reads_and_searches_the_current_project_before_answering() {
    let fixture = NativeTerminalFixture::new("structured-local-tools");
    std::fs::create_dir_all(fixture.project.join("src")).unwrap();
    std::fs::write(
        fixture.project.join("src/lib.rs"),
        "pub const LOCAL_TOOL_MARKER: &str = \"project-scoped\";\n",
    )
    .unwrap();
    std::fs::write(
        fixture.project.join("README.md"),
        "# local tool fixture\n\nThe source marker lives in src/lib.rs.\n",
    )
    .unwrap();

    let backend = fixture.start_conversation_backend_with_structured_sequence_and_text(
        &[
            r#"{"decision":"local_task","input":"","answer":""}"#,
            r#"{"decision":"read_file","input":"src/lib.rs","answer":""}"#,
            r#"{"decision":"search_repository","input":"LOCAL_TOOL_MARKER","answer":""}"#,
            r#"{"decision":"answer","input":"","answer":""}"#,
        ],
        "src/lib.rs에서 LOCAL_TOOL_MARKER 상수를 확인했습니다.",
    );

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    let mark = terminal.mark();
    submit_visible_command(
        &mut terminal,
        "현재 프로젝트의 src/lib.rs를 읽고 LOCAL_TOOL_MARKER가 있는 위치를 찾아서 알려줘",
    );
    let turn = terminal.wait_for_after(
        mark,
        "src/lib.rs에서 LOCAL_TOOL_MARKER 상수를 확인했습니다.",
    );
    assert!(!turn.contains(r#"{"decision":"read_file""#));
    assert!(!turn.contains("RUNTIME_LOCAL_OBSERVATIONS"));
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();

    let requests = backend.request_bodies();
    assert_eq!(requests.len(), 5, "local tool requests: {requests:#?}");
    assert!(requests[0].contains("local_task"));
    assert!(requests[1].contains("read_file"));
    assert!(requests[1].contains("search_repository"));
    assert!(!requests[1].contains("web_search"));
    assert!(requests[2].contains("RUNTIME_LOCAL_OBSERVATIONS"));
    assert!(requests[2].contains("LOCAL_TOOL_MARKER"));
    assert!(requests[2].contains("project-scoped"));
    assert!(requests[3].contains("RUNTIME_LOCAL_OBSERVATIONS"));
    assert!(requests[3].contains("src/lib.rs"));
    assert!(requests[3].contains("LOCAL_TOOL_MARKER"));
    assert!(requests[3].contains("\"response_format\""));
    assert!(!requests[3].contains(r#""answer":{"type":"string""#));
    assert!(requests[4].contains("RUNTIME_LOCAL_OBSERVATIONS"));
    assert!(requests[4].contains("src/lib.rs"));
    assert!(requests[4].contains("LOCAL_TOOL_MARKER"));
    assert!(!requests[4].contains("\"response_format\""));
}
