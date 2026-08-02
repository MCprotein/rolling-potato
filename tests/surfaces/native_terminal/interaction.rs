use super::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn sessions_start_fresh_and_resume_only_through_the_explicit_picker() {
    let fixture = NativeTerminalFixture::new("explicit-session-resume");
    assert!(fixture.project.is_dir());
    let _live_terminal = LiveTerminalEnvironment::enable();

    let mut terminal = NativePty::spawn(120, 40);
    let initial = terminal.wait_for("session new");
    assert!(initial.contains("새 대화"));
    terminal.send("/new\n");
    terminal.wait_for("새 세션을 시작했습니다.");
    terminal.send("/quit\n");
    terminal.finish();

    let mut terminal = NativePty::spawn(120, 40);
    let restarted = terminal.wait_for("session new");
    assert!(
        !restarted.contains("새 세션을 시작했습니다."),
        "저장된 세션은 명시적으로 선택하기 전에 자동 복원되면 안 됩니다."
    );
    terminal.send("/resume\n");
    let picker = terminal.wait_for("세션 재개");
    assert!(
        picker.contains("session-"),
        "세션 선택 항목이 보이지 않습니다:\n{picker}"
    );
    terminal.send("1");
    terminal.wait_for("선택한 세션을 재개했습니다.");
    terminal.send("/quit\n");
    terminal.finish();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn slash_opens_command_palette_before_enter() {
    let fixture = NativeTerminalFixture::new("slash-command-palette");
    assert!(fixture.project.is_dir());
    let _live_terminal = LiveTerminalEnvironment::enable();

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    terminal.send("/");
    let palette = terminal.wait_for("/model [id]");
    assert!(palette.contains("모델 확인 및 변경"));
    assert!(palette.contains("/compact"));

    terminal.send("model\n");
    let picker = terminal.wait_for("Gemma 4 E4B IT QAT Q4_0 GGUF");
    assert!(picker.contains("모델 선택"));
    assert!(picker.contains("Qwen3.5 4B Q4_K_M GGUF"));
    assert!(picker.contains("Gemma 4 E4B IT QAT Q4_0 GGUF"));
    let picker_close = terminal.mark();
    terminal.send("\u{1b}");
    terminal.wait_for_after(picker_close, "╭─ rpotato v");
    terminal.send("/없는명령\n");
    terminal.wait_for("알 수 없는 TUI 명령입니다: /없는명령");
    terminal.send("/helx\u{7f}p\n");
    terminal.wait_for("고급 호환 명령: rpotato debug --help");
    terminal.send("/");
    terminal.wait_for("/compact");
    terminal.send("\u{7f}");
    terminal.send("\u{1a}");
    terminal.send("\u{1b}[3~");
    terminal.send("/quit\n");
    let output = terminal.finish();
    assert!(
        output
            .matches("고급 호환 명령: rpotato debug --help")
            .count()
            >= 2,
        "palette dismissal must restore the overwritten conversation rows"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn page_scroll_preserves_the_live_composer_draft() {
    let fixture = NativeTerminalFixture::new("scroll-preserves-draft");
    assert!(fixture.project.is_dir());
    let _live_terminal = LiveTerminalEnvironment::enable();

    let mut terminal = NativePty::spawn(80, 12);
    terminal.wait_for("›");
    for _ in 1..=8 {
        let mark = terminal.mark();
        submit_visible_command(&mut terminal, "넌 누구야");
        terminal.wait_for_after(mark, "저는 로컬에서 실행되는");
    }

    terminal.send("/qu");
    terminal.wait_for("/qu");
    let scroll_mark = terminal.mark();
    terminal.send("\u{1b}[5~");
    terminal.wait_for_after(scroll_mark, "↑ 이전 대화");
    terminal.wait_for_after(scroll_mark, "/qu");

    terminal.send("it\r");
    let output = terminal.finish();
    assert!(!output.contains("알 수 없는 TUI 명령"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn bracketed_clipboard_image_path_is_captured_before_slash_command_routing() {
    let fixture = NativeTerminalFixture::new("clipboard-image-path");
    let _live_terminal = LiveTerminalEnvironment::enable();
    let image = fixture.root.join("clipboard-test.png");
    std::fs::write(&image, b"\x89PNG\r\n\x1a\nfixture").unwrap();

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    let pasted = format!("{}\u{2060}", image.display());
    terminal.send(&format!("\u{1b}[200~{pasted}\u{1b}[201~\r"));
    let output = terminal.wait_for("첨부됨");
    assert!(output.contains("clipboard-test.png"));
    assert!(!output.contains("알 수 없는 TUI 명령"));
    terminal.send("/attach clear\r");
    terminal.wait_for("대기 중인 첨부를 모두 제거했습니다.");
    terminal.send("/quit\r");
    terminal.finish();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn update_uses_a_live_confirmation_picker_without_free_form_yes_input() {
    let fixture = NativeTerminalFixture::new("update-confirmation-picker");
    assert!(fixture.project.is_dir());
    std::env::set_var(
        "RPOTATO_TEST_UPDATE_REPORT",
        "rpotato update\n- status: updated\n- installed: v9.0.0",
    );
    let _live_terminal = LiveTerminalEnvironment::enable();

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    terminal.send("/update\n");
    let picker = terminal.wait_for("2. 업데이트 시작");
    assert!(picker.contains("업데이트 확인"));
    assert!(picker.contains("1. 취소"));
    assert!(!picker.contains("yes를 입력"));
    terminal.send("\n");
    terminal.wait_for("업데이트를 취소했습니다.");
    let second_picker_mark = terminal.mark();
    terminal.send("/update\n");
    terminal.wait_for_after(second_picker_mark, "2. 업데이트 시작");
    terminal.send("2");
    terminal.wait_for("installed: v9.0.0");
    terminal.send("/quit\n");
    let output = terminal.finish();

    std::env::remove_var("RPOTATO_TEST_UPDATE_REPORT");
    assert!(output.contains("status: updated"));
    assert!(!output.contains("yes를 입력"));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn ctrl_c_cancels_the_active_request_without_leaking_backend_details() {
    let fixture = NativeTerminalFixture::new("active-request-cancel");
    let backend = fixture
        .start_conversation_backend_with_responses("__RPOTATO_STALL__", "사용되지 않는 응답");
    let _live_terminal = LiveTerminalEnvironment::enable();

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("local ready");
    let request_mark = terminal.mark();
    submit_visible_command(&mut terminal, "오래 걸리는 요청");
    terminal.wait_for_after(request_mark, "Ctrl+C 취소");
    terminal.send_signal(2);
    let cancelled = terminal.wait_for_after(request_mark, "요청을 취소했습니다.");
    assert!(!cancelled.contains("generation id"));
    assert!(!cancelled.contains("sidecar pid"));
    assert!(!cancelled.contains("runtime ledger"));

    backend.set_structured_response(
        r#"{"decision":"answer","input":"","answer":"취소 후에도 정상 응답합니다."}"#,
    );
    let recovery_mark = terminal.mark();
    submit_visible_command(&mut terminal, "다시 답해줘");
    terminal.wait_for_after(recovery_mark, "취소 후에도 정상 응답합니다.");
    submit_visible_command(&mut terminal, "/quit");
    terminal.finish();
}
