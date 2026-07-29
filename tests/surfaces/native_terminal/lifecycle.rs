use super::*;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn entry_quit() {
    let fixture = NativeTerminalFixture::new("entry-quit");
    assert!(fixture.project.is_dir());
    assert!(fixture.data.is_dir());
    assert_eq!(
        std::env::var_os("RPOTATO_TEST_SKIP_UPDATE_CHECK").as_deref(),
        Some(std::ffi::OsStr::new("1")),
        "native terminal fixtures must not depend on the live release API"
    );
    let before = tree_snapshot(&[&fixture.project, &fixture.data]);

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("session new");
    let first = terminal.wait_for("›");
    assert!(first.contains("로컬 코딩 에이전트"));
    assert!(first.contains("╭─ rpotato v"));
    assert!(first.contains("│ model"));
    assert!(first.contains("local stopped"));
    terminal.send("quit\n");
    let output = terminal.finish();
    assert!(!output.contains("terminal.capability"));

    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("session new");
    let second = terminal.wait_for("›");
    assert!(second.contains("로컬 코딩 에이전트"));
    terminal.send_eof();
    let output = terminal.finish();
    assert!(!output.contains("terminal.capability"));

    assert_tree_unchanged(
        &before,
        &tree_snapshot(&[&fixture.project, &fixture.data]),
        "quit and EOF zero-delta entry",
    );
}

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn entry_switches_projects_without_manual_state_reconcile() {
    let fixture = NativeTerminalFixture::new("entry-project-switch");
    let legacy_dir = fixture.data.join("state");
    std::fs::create_dir_all(&legacy_dir).unwrap();
    std::fs::rename(
        fixture.project.join(".rpotato/state/current-state.json"),
        legacy_dir.join("current-state.json"),
    )
    .unwrap();
    let next_project = fixture.root.join("next-project");
    std::fs::create_dir_all(&next_project).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &next_project);

    let mut terminal = NativePty::spawn(120, 40);
    let first = terminal.wait_for("›");
    assert!(first.contains("로컬 코딩 에이전트"));
    terminal.send("quit\n");
    let output = terminal.finish();

    assert!(!output.contains("응답 언어 검증에 실패했습니다"));
    assert!(next_project
        .join(".rpotato/state/current-state.json")
        .is_file());
    assert!(legacy_dir.join("current-state.json").is_file());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn secret_prompt_restores_echo_before_sigint_and_sigterm_exit() {
    let fixture = NativeTerminalFixture::new("secret-signal-restore");
    std::env::set_var("RPOTATO_TEST_TUI_SECRET_PROBE", "1");
    let before = tree_snapshot(&[&fixture.project, &fixture.data]);

    for signal in [2, 15] {
        let mut terminal = NativePty::spawn(120, 40);
        terminal.wait_for("›");
        terminal.send("test-secret\n");
        terminal.wait_for("비밀 probe를 무반향으로 입력하세요.");
        terminal.send_signal(signal);
        let output = terminal.finish_failure();
        assert!(!output.contains("terminal.echo-restore.failed"));
        assert_tree_unchanged(
            &before,
            &tree_snapshot(&[&fixture.project, &fixture.data]),
            "secret signal restoration",
        );
        assert_clean_restart();
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn pty_drop_escalates_when_child_cannot_handle_sigterm() {
    let _fixture = NativeTerminalFixture::new("bounded-pty-drop");
    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    terminal.force_drop_escalation_probe();

    let started = std::time::Instant::now();
    drop(terminal);

    assert!(
        started.elapsed() < std::time::Duration::from_secs(3),
        "PTY drop exceeded its bounded termination budget: {:?}",
        started.elapsed()
    );
}
