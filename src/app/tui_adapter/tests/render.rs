#[test]
fn interactive_sanitizer_escapes_ansi_osc_and_control_bytes() {
    let hostile = "safe\u{001b}[31mred\u{001b}[0m\u{001b}]0;title\u{0007}\nnext\u{0000}";
    let sanitized = sanitize_terminal_text(hostile);

    assert_eq!(sanitized, "safe<esc>red<esc><esc><lf>next<ctl>");
    assert!(!sanitized.contains('\u{001b}'));
    assert!(!sanitized.contains('\u{0000}'));
}

#[test]
fn exact_outcome_notice_preserves_trusted_multiline_structure() {
    let state = InteractiveState {
        view: InteractiveView::Overview,
        notice: "결과 제목\n- code: exact.test\n- 동작: 상태를 변경하지 않았습니다.".to_string(),
        ..InteractiveState::new()
    };
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: Vec::new(),
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Complete,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame(&state, &page, 120, 40);

    assert!(frame.contains(
        "notice: 결과 제목\n        - code: exact.test\n        - 동작: 상태를 변경하지 않았습니다.\n"
    ));
    assert!(!frame.contains("<lf>"));
}

#[test]
fn interactive_status_bar_uses_real_metric_labels_below_the_ansi_input_line() {
    let state = InteractiveState::new();
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: Vec::new(),
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Complete,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };
    let mut status = TuiStatusSnapshot {
        model: "gemma-4-e4b".to_string(),
        context_tokens_used: Some(1024),
        context_limit_tokens: Some(4096),
        has_compaction_checkpoint: false,
        backend: TuiBackendStatus::Ready,
        vision: crate::surfaces::tui::runtime_bridge::TuiVisionStatus::OnDemand,
        session_id: "session-long-identifier".to_string(),
    };

    let frame = render_interactive_frame_with_options(&state, &page, &status, 120, 40, true, true);

    let prompt = frame.find("› ").unwrap();
    let status_line = frame.find("model gemma-4-e4b").unwrap();
    assert!(prompt < status_line);
    assert!(frame.contains("ctx 1024/4096 (25%)"));
    assert!(frame.contains("compact auto@75%"));
    assert!(frame.contains("local ready"));
    assert!(frame.contains("vision on-demand") && !frame.contains("vision text-only"));
    assert!(frame.contains("\u{001b}[36mmodel gemma-4-e4b"));
    assert!(frame.contains("\u{001b}[32mlocal ready"));
    assert!(frame.contains("╭─ 요청 "));
    assert!(frame.ends_with("\n\u{001b}[3A\r\u{001b}[4C"));

    status.has_compaction_checkpoint = true;
    let saved = render_interactive_frame_with_options(&state, &page, &status, 120, 40, true, true);
    assert!(saved.contains("compact saved"));

    status.has_compaction_checkpoint = false;
    status.context_tokens_used = Some(3072);
    let due = render_interactive_frame_with_options(&state, &page, &status, 120, 40, true, true);
    assert!(due.contains("compact due"));
}

#[test]
fn no_color_forces_a_plain_frame_without_layout_escape_sequences() {
    let state = InteractiveState::new();
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: vec!["body".to_string()],
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Complete,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame_with_options(
        &state,
        &page,
        &TuiStatusSnapshot::unavailable(),
        80,
        24,
        true,
        false,
    );

    assert!(!frame.contains('\u{001b}'));
    assert!(frame.contains("local unavailable"));
}

#[test]
fn long_notice_keeps_composer_and_status_inside_the_terminal_row_budget() {
    let mut state = InteractiveState::new();
    state.notice = (0..20)
        .map(|index| format!("notice line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: (0..20).map(|index| format!("body {index}")).collect(),
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Complete,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame_with_options(
        &state,
        &page,
        &TuiStatusSnapshot::unavailable(),
        80,
        10,
        true,
        true,
    );

    assert!(frame.find("› ").unwrap() < frame.find("model 미선택").unwrap());
    assert!(frame.matches('\n').count() < 10);
    assert!(frame.contains("…"));
    assert!(frame.ends_with("\n\u{001b}[3A\r\u{001b}[4C"));
}

#[test]
fn conversation_wraps_long_korean_turns_without_losing_content() {
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        "한국어 응답이 좁은 터미널에서도 입력창을 밀어내지 않고 다음 줄에 계속 표시됩니다.",
    );

    let frame = render_interactive_frame(&state, &TuiReadPage::conversation_placeholder(), 20, 14);

    assert!(frame.contains("한국어 응답이 좁은"));
    assert!(frame.contains("다음 줄에 계속"));
    assert!(frame.contains("니다."));
    assert!(
        frame.lines().all(|line| display_cell_width(line) <= 20),
        "wrapped frame exceeded terminal cell width:\n{frame}"
    );
}

#[test]
fn long_conversation_pages_keep_every_response_line_reachable() {
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        (0..20)
            .map(|index| format!("response line {index}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    let page = TuiReadPage::conversation_placeholder();
    let page_count = conversation_page_count(&state, 40, 10);
    assert!(page_count > 1);

    let latest = render_interactive_frame(&state, &page, 40, 10);
    assert!(latest.contains("response line 19"));
    assert!(!latest.contains("response line 0"));

    let mut all_pages = latest;
    for _ in 1..page_count {
        state.next_notice_page(10, page_count);
        all_pages.push_str(&render_interactive_frame(&state, &page, 40, 10));
    }
    for index in 0..20 {
        assert!(
            all_pages.contains(&format!("response line {index}")),
            "conversation page lost response line {index}"
        );
    }
    let oldest = render_interactive_frame(&state, &page, 40, 10);
    assert!(oldest.contains("● response line 0"));
    assert!(oldest.contains("↑ 이전 대화"));
    assert!(oldest.contains("PageDown/휠↓ 최신"));
    let oldest_page = state.notice_page;
    state.previous_notice_page();
    assert_eq!(state.notice_page, oldest_page - 1);
}

#[test]
fn long_notice_pages_preserve_later_response_lines() {
    let mut state = InteractiveState::new();
    state.notice = (0..20)
        .map(|index| format!("response line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    for _ in 0..20 {
        state.next_notice_page(10, 1);
    }
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: Vec::new(),
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Complete,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame(&state, &page, 80, 10);

    assert!(frame.contains("response line 19"));
    assert!(!frame.contains("response line 0"));
    let last_page = state.notice_page;
    state.previous_notice_page();
    assert_eq!(state.notice_page, last_page - 1);
}
