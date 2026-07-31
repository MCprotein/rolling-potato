use super::*;

#[test]
fn pasted_image_path_becomes_an_attachment_instead_of_an_unknown_command() {
    let path = "/private/tmp/rpotato-screen.png";
    let mut terminal = ScriptedTerminal::new([path, "이 이미지 봐줘", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.captured_paths, [path]);
    assert_eq!(runtime.requests, ["이 이미지 봐줘"]);
    assert_eq!(runtime.submitted_attachment_counts, [1]);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("[image:"));
    assert!(!rendered.contains("알 수 없는 TUI 명령"));
}

#[test]
fn failed_request_keeps_attachments_until_a_successful_retry() {
    let path = "/private/tmp/rpotato-retry.png";
    let mut terminal =
        ScriptedTerminal::new([path, "첫 요청", "재시도", "첨부 없는 요청", "/quit"]);
    let mut runtime = ConversationRuntime {
        submit_failures_remaining: 1,
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.requests, ["첫 요청", "재시도", "첨부 없는 요청"]);
    assert_eq!(runtime.submitted_attachment_counts, [1, 1, 0]);
    assert!(terminal
        .frames
        .join("\n")
        .contains("첨부는 재시도를 위해 유지했습니다. 제거하려면 /attach clear를 사용하세요."));
}

#[test]
fn pending_attachments_can_be_cleared_without_erasing_the_conversation() {
    let path = "/private/tmp/rpotato-clear.png";
    let mut terminal = ScriptedTerminal::new([path, "/attach clear", "첨부 없는 요청", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.requests, ["첨부 없는 요청"]);
    assert_eq!(runtime.submitted_attachment_counts, [0]);
    assert!(terminal
        .frames
        .join("\n")
        .contains("대기 중인 첨부를 모두 제거했습니다."));
}

#[test]
fn failed_request_is_rendered_as_an_error_instead_of_a_green_assistant_turn() {
    let mut terminal = ScriptedTerminal::new(["실패 요청", "/quit"]);
    let mut runtime = ConversationRuntime {
        submit_failures_remaining: 1,
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("× 요청을 완료하지 못했습니다."));
    assert!(!rendered.contains("● 요청을 완료하지 못했습니다."));
}

#[test]
fn wide_terminal_uses_the_full_viewport_for_chrome_and_a_bounded_reading_column() {
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        "긴 답변은 읽기 좋은 폭으로 유지하지만 입력창은 터미널 전체를 사용합니다.",
    );

    let frame = render_interactive_frame_with_options(
        &state,
        &TuiReadPage::conversation_placeholder(),
        &TuiStatusSnapshot::unavailable(),
        160,
        40,
        true,
        true,
    );
    let visible = strip_ansi(&frame);
    let composer = visible
        .lines()
        .find(|line| line.contains("─ 요청 "))
        .expect("composer top rule");
    assert_eq!(display_cell_width(composer), 160);
    let transcript = visible.split("╭─ 요청").next().expect("transcript");
    assert!(transcript
        .lines()
        .filter(|line| line.starts_with("● ") || line.starts_with("│ "))
        .all(|line| display_cell_width(line) <= 120));
}

#[test]
fn conversation_frame_sanitizes_project_path_and_respects_terminal_cell_width() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let previous_root = std::env::var_os("RPOTATO_PROJECT_ROOT");
    std::env::set_var("RPOTATO_PROJECT_ROOT", "/\u{001b}[31m위험\n프로젝트");
    let mut state = InteractiveState::new();
    state.push_turn(
        ConversationRole::Assistant,
        "한국어 응답이 좁은 터미널에서도 입력창을 밀어내지 않습니다.",
    );
    state.notice =
        "웹 조사 · 검색 중\n검색 ● → 결과 평가 ○ → 문서 읽기 ○ → 증거 구성 ○ → 답변 ○".to_string();

    let frame = render_interactive_frame(&state, &TuiReadPage::conversation_placeholder(), 40, 12);

    if let Some(previous_root) = previous_root {
        std::env::set_var("RPOTATO_PROJECT_ROOT", previous_root);
    } else {
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
    }
    assert!(!frame.contains('\u{001b}'));
    assert!(frame.contains("<esc>"));
    assert!(!frame.contains("\n프로젝트"));
    assert!(
        frame.lines().all(|line| display_cell_width(line) <= 40),
        "narrow frame exceeded terminal cell width:\n{frame}"
    );
    assert!(frame.ends_with("› "));
}
