use super::*;

#[test]
fn default_interactive_frame_is_conversation_first_and_hides_runtime_internals() {
    let state = InteractiveState::new();
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: vec![
            "current: revision=21 hash=secret-current-hash".to_string(),
            "ledger: sequence=145 hash=secret-ledger-hash".to_string(),
            "projected workflows: 1".to_string(),
        ],
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Truncated,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame(&state, &page, 120, 40);

    assert!(frame.contains("╭─ rpotato v"));
    assert!(frame.contains("│ model"));
    assert!(frame.contains("│ project"));
    assert!(frame.contains("╰─ /help 명령 · /model 변경"));
    assert!(frame.contains("› "));
    let welcome_footer = frame
        .lines()
        .position(|line| line.contains("╰─ /help 명령 · /model 변경"))
        .expect("welcome footer");
    assert_eq!(
        frame.lines().nth(welcome_footer + 1),
        Some(""),
        "welcome footer and first notice need one blank row"
    );
    for hidden in [
        "freshness",
        "continuation",
        "secret-current-hash",
        "secret-ledger-hash",
        "projected workflows",
    ] {
        assert!(!frame.contains(hidden), "default frame leaked {hidden}");
    }
}

#[test]
fn ordinary_input_renders_as_user_and_assistant_turns() {
    let mut terminal = ScriptedTerminal::new(["안녕", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert_eq!(runtime.requests, ["안녕"]);
    assert_eq!(runtime.page_reads, 0, "default chat must not read overview");
    assert!(terminal.frames[1].contains("› 안녕"));
    assert!(!terminal.frames[1].contains("● 안녕하세요."));
    assert!(terminal.frames[1].contains("◇ ⠋ 처리 중"));
    assert!(!terminal.frames[1].contains("notice:"));
    assert!(rendered.contains("› 안녕"));
    assert!(rendered.contains("● 안녕하세요."));
    assert!(!rendered.contains("ledger: must stay hidden"));
    assert!(!rendered.contains("patch proposal"));
}

#[test]
fn browser_request_renders_structured_progress_before_synchronous_execution() {
    let mut terminal = ScriptedTerminal::new(["네이버를 열고 검색창에 월드컵을 입력해", "/quit"]);
    let mut runtime = ConversationRuntime {
        progress_hint: Some(
            "브라우저 조사 · 공개 검색 페이지 여는 중\n페이지 열기 ● → 검색창 확인 ○ → 검색어 입력 ○ → 결과 읽기 ○"
                .to_string(),
        ),
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert!(terminal.frames[1].contains("브라우저 조사"));
    assert!(terminal.frames[1].contains("페이지 열기 ●"));
    assert!(terminal.frames[1].contains("결과 읽기 ○"));
}

#[test]
fn explicit_browser_request_uses_small_model_fallback_before_inference() {
    let decision =
        conversation::decide_request("네이버를 열고 검색창에 월드컵을 입력해", &[], 4096, true)
            .unwrap();

    let conversation::RequestDecision::BrowserTool(request) = decision else {
        panic!("explicit browser request did not route to BrowserTool");
    };
    assert_eq!(request.url, "https://www.naver.com/");
    assert_eq!(request.query, "월드컵");
}

#[test]
fn ansi_conversation_distinguishes_failures_from_assistant_answers() {
    let mut state = InteractiveState::new();
    state.push_turn(ConversationRole::Assistant, "정상 답변");
    state.push_turn(ConversationRole::Error, "복구 가능한 오류");

    let frame = render_interactive_frame_with_options(
        &state,
        &TuiReadPage::conversation_placeholder(),
        &TuiStatusSnapshot::unavailable(),
        120,
        40,
        true,
        true,
    );

    assert!(frame.contains("\u{001b}[1;32m● \u{001b}[0m정상 답변"));
    assert!(frame.contains("\u{001b}[31m× \u{001b}[0m복구 가능한 오류"));
    assert!(!frame.contains("\u{001b}[1;32m× "));
}

#[test]
fn assistant_markdown_is_presented_without_literal_emphasis_markers() {
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        "## 비교\n* **Qwen**: `코딩`에 강함\n* **Gemma**: 대화에 강함",
    );

    let frame = render_interactive_frame_with_options(
        &state,
        &TuiReadPage::conversation_placeholder(),
        &TuiStatusSnapshot::unavailable(),
        120,
        40,
        true,
        true,
    );

    assert!(frame.contains("비교"));
    assert!(frame.contains("• "));
    assert!(frame.contains("\u{001b}[1mQwen\u{001b}[22m"));
    assert!(frame.contains("\u{001b}[36m코딩\u{001b}[0m"));
    assert!(!strip_ansi(&frame).contains("**"));
}
