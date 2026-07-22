use crate::native_terminal_support::{self, tree_snapshot, NativeTerminalFixture};

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
use crate::native_terminal_support::NativePty;

#[cfg(windows)]
use crate::native_terminal_support::trace_stage;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn entry_quit() {
    let fixture = NativeTerminalFixture::new("entry-quit");
    assert!(fixture.project.is_dir());
    assert!(fixture.data.is_dir());
    let before = tree_snapshot(&[&fixture.project, &fixture.data]);

    let mut terminal = NativePty::spawn(120, 40);
    let first = terminal.wait_for("›");
    assert!(first.contains("로컬 코딩 에이전트"));
    terminal.send("quit\n");
    let output = terminal.finish();
    assert!(!output.contains("terminal.capability"));

    let mut terminal = NativePty::spawn(120, 40);
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

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn full_adapter() {
    #[cfg(windows)]
    trace_stage("full_adapter start");
    let fixture = NativeTerminalFixture::new("full-adapter");
    #[cfg(windows)]
    trace_stage("fixture initialized");
    let pending = fixture.prepare_source_approval();
    #[cfg(windows)]
    trace_stage("source approval prepared");
    #[cfg(unix)]
    let before_ledger = runtime_ledger(&fixture);
    #[cfg(unix)]
    let before_workflow_revision = json_u64(
        &std::fs::read_to_string(
            fixture
                .project
                .join(".rpotato/workflows")
                .join(format!("{}.json", pending.workflow_id)),
        )
        .unwrap(),
        "committed_revision",
    );
    #[cfg(unix)]
    let before_current_revision = json_u64(
        &std::fs::read_to_string(fixture.project.join(".rpotato/state/current-state.json"))
            .unwrap(),
        "revision",
    );
    std::env::set_var("RPOTATO_TEST_TUI_SECRET_PROBE", "1");

    for (fault, code, requires_prompt) in [
        ("invalid-fault-value", "InvalidFaultConfiguration", false),
        ("size-read", "terminal.capability.size-read", false),
        ("mode-read", "terminal.capability.mode-read", true),
        ("no-echo-set", "terminal.no-echo-set.failed", true),
        ("secret-read", "terminal.secret-read.failed", true),
    ] {
        let fault_before = tree_snapshot(&[&fixture.project, &fixture.data]);
        std::env::set_var("RPOTATO_TEST_TERMINAL_FAULT", fault);
        let mut terminal = NativePty::spawn(120, 40);
        if requires_prompt {
            terminal.wait_for("›");
            terminal.send("test-secret\n");
        }
        terminal.wait_for(code);
        let output = terminal.finish_failure();
        std::env::remove_var("RPOTATO_TEST_TERMINAL_FAULT");
        native_terminal_fault_outcomes_exact(fault, &output);
        assert!(output.contains(code), "missing fault result for {fault}");
        assert!(!output.contains(&pending.approval_token));
        assert_tree_unchanged(
            &fault_before,
            &tree_snapshot(&[&fixture.project, &fixture.data]),
            &format!("terminal fault {fault}"),
        );
        assert_clean_restart();
    }

    std::env::set_var("RPOTATO_TEST_TERMINAL_FAULT", "frame-write-before-dispatch");
    let frame_before_snapshot = tree_snapshot(&[&fixture.project, &fixture.data]);
    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    #[cfg(unix)]
    {
        terminal.send(&format!("select {}\n", pending.workflow_id));
        terminal.wait_for(&format!("선택: {}", pending.workflow_id));
        terminal.send(&format!("approve {}\n", pending.proposal_id));
        terminal.wait_for("패치 적용 승인을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
    }
    #[cfg(windows)]
    {
        let session_id = fixture.current_session_id();
        terminal.send(&format!("select session {session_id}\n"));
        terminal.wait_for("세션 선택을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
    }
    terminal.wait_for("terminal.frame-write.pre-dispatch");
    let output = terminal.finish_failure();
    std::env::remove_var("RPOTATO_TEST_TERMINAL_FAULT");
    native_terminal_fault_outcomes_exact("frame-write-before-dispatch", &output);
    assert!(output.contains("terminal.frame-write.pre-dispatch"));
    assert!(!output.contains(&pending.approval_token));
    assert_tree_unchanged(
        &frame_before_snapshot,
        &tree_snapshot(&[&fixture.project, &fixture.data]),
        "pre-dispatch frame failure",
    );
    assert_clean_restart();

    #[cfg(windows)]
    let before_snapshot = tree_snapshot(&[&fixture.project, &fixture.data]);
    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    terminal.resize(80, 24);
    terminal.send("help\n");
    let help = terminal.wait_for("/model [id]");
    assert!(help.contains("rpotato v"));
    terminal.resize(40, 10);
    terminal.send("view sessions\n");
    let sessions = terminal.wait_for("rpotato | sessions");
    assert!(sessions.contains("freshness fresh"));
    terminal.resize(120, 40);
    terminal.send("test-secret\n");
    terminal.wait_for("비밀 probe를 무반향으로 입력하세요.");
    let secret = "NATIVE_SECRET_MUST_NOT_ECHO_7341";
    terminal.send(&format!("{secret}\n"));
    terminal.wait_for("secret.refresh-only");
    terminal.send(&format!("select {}\n", pending.workflow_id));
    terminal.wait_for(&format!("선택: {}", pending.workflow_id));
    terminal.send(&format!("approve {}\n", pending.proposal_id));
    #[cfg(unix)]
    {
        terminal.wait_for("패치 적용 승인을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
        terminal.wait_for("토큰을 무반향으로 입력하세요.");
        terminal.send(&format!("{}\n", pending.approval_token));
        terminal.wait_for("verification.credential-issued");
    }
    #[cfg(windows)]
    terminal.wait_for("source-install.unsupported-platform");
    let output = terminal.wait_for("›");

    #[cfg(unix)]
    {
        assert_unix_approval_oracle(
            &fixture,
            &pending,
            &before_ledger,
            before_workflow_revision,
            before_current_revision,
        );
        let credential = output
            .split("verification credential (one-time): ")
            .nth(1)
            .and_then(|tail| {
                tail.split(|character: char| !character.is_ascii_hexdigit())
                    .find(|value| value.len() == 64)
            })
            .expect("one-time verification credential must be rendered once");
        assert!(!tree_contains(&fixture.project, credential.as_bytes()));
        assert!(!tree_contains(&fixture.data, credential.as_bytes()));
        terminal.send("deny\n");
        terminal.wait_for("상태 변경을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
        let denial_output = terminal.wait_for("다음: 롤백 영수증을 확인하세요.");
        native_terminal_denial_block_outcomes_exact(
            &denial_output,
            "deny.verification.rolled-back",
            &pending.workflow_id,
            None,
        );
        assert_eq!(
            std::fs::read_to_string(&pending.source).unwrap(),
            "pub const VALUE: i32 = 1;\n"
        );
        // The denial oracle above proves rollback. Restore this terminal fixture's
        // ontology-bound source so a later, independent canonical workflow can rebuild
        // context without inheriting a deliberately rolled-back graph/source mismatch.
        std::fs::write(&pending.source, "pub const VALUE: i32 = 2;\n").unwrap();
        terminal.send("deny\n");
        terminal.wait_for("상태 변경을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
        let terminal_denial = terminal.wait_for("다음: 기존 종료 영수증을 확인하세요.");
        native_terminal_denial_block_outcomes_exact(
            &terminal_denial,
            "deny.blocked.terminal-state",
            &pending.workflow_id,
            Some("cancelled"),
        );
    }

    #[cfg(windows)]
    {
        assert_eq!(
            std::fs::read_to_string(&pending.source).unwrap(),
            "pub const VALUE: i32 = 1;\n"
        );
        assert!(output.contains("source-install.unsupported-platform"));
        assert_tree_unchanged(
            &before_snapshot,
            &tree_snapshot(&[&fixture.project, &fixture.data]),
            "unsupported source approval",
        );
        terminal.send("deny\n");
        terminal.wait_for("상태 변경을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
        let denial_output = terminal.wait_for("다음: 거부 영수증을 확인하세요.");
        native_terminal_denial_block_outcomes_exact(
            &denial_output,
            "deny.patch.accepted",
            &pending.workflow_id,
            None,
        );
        terminal.send("deny\n");
        terminal.wait_for("상태 변경을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
        let terminal_denial = terminal.wait_for("다음: 기존 종료 영수증을 확인하세요.");
        native_terminal_denial_block_outcomes_exact(
            &terminal_denial,
            "deny.blocked.terminal-state",
            &pending.workflow_id,
            Some("cancelled"),
        );
    }

    terminal.send("quit\n");
    let output = terminal.finish();
    assert!(!output.contains(secret));
    assert!(!output.contains(&pending.approval_token));
    assert!(!output.contains("terminal.no-echo-set.failed"));
    assert!(!output.contains("terminal.frame-write"));
    assert!(!tree_contains(&fixture.project, secret.as_bytes()));
    assert!(!tree_contains(&fixture.data, secret.as_bytes()));
    assert!(!tree_contains(
        &fixture.project,
        pending.approval_token.as_bytes()
    ));
    assert!(!tree_contains(
        &fixture.data,
        pending.approval_token.as_bytes()
    ));

    let post = fixture.prepare_source_approval();
    let post_before_ledger = runtime_ledger(&fixture);
    #[cfg(unix)]
    let post_before_revision = json_u64(
        &std::fs::read_to_string(
            fixture
                .project
                .join(".rpotato/workflows")
                .join(format!("{}.json", post.workflow_id)),
        )
        .unwrap(),
        "committed_revision",
    );
    #[cfg(unix)]
    let post_before_current_revision = json_u64(
        &std::fs::read_to_string(fixture.project.join(".rpotato/state/current-state.json"))
            .unwrap(),
        "revision",
    );
    #[cfg(windows)]
    let post_before_snapshot = tree_snapshot(&[&fixture.project, &fixture.data]);
    std::env::set_var("RPOTATO_TEST_TERMINAL_FAULT", "frame-write-after-dispatch");
    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    terminal.send(&format!("select {}\n", post.workflow_id));
    terminal.wait_for(&format!("선택: {}", post.workflow_id));
    terminal.send(&format!("approve {}\n", post.proposal_id));
    #[cfg(unix)]
    {
        terminal.wait_for("패치 적용 승인을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
        terminal.wait_for("토큰을 무반향으로 입력하세요.");
        terminal.send(&format!("{}\n", post.approval_token));
        terminal.wait_for("terminal.frame-write.post-dispatch");
    }
    #[cfg(windows)]
    {
        terminal.wait_for("source-install.unsupported-platform");
        assert_tree_unchanged(
            &post_before_snapshot,
            &tree_snapshot(&[&fixture.project, &fixture.data]),
            "unsupported source action before post-dispatch boundary",
        );
        let session_id = fixture.current_session_id();
        terminal.send(&format!("select session {session_id}\n"));
        terminal.wait_for("세션 선택을 확인하려면 yes를 입력하세요.");
        terminal.send("yes\n");
        terminal.wait_for("terminal.frame-write.post-dispatch");
    }
    let output = terminal.finish_failure();
    std::env::remove_var("RPOTATO_TEST_TERMINAL_FAULT");
    native_terminal_fault_outcomes_exact("frame-write-after-dispatch", &output);
    assert!(output.contains("terminal.frame-write.post-dispatch"));
    assert!(!output.contains(&post.approval_token));
    assert!(!output.contains("verification credential (one-time):"));
    assert!(!tree_contains(
        &fixture.project,
        post.approval_token.as_bytes()
    ));
    assert!(!tree_contains(
        &fixture.data,
        post.approval_token.as_bytes()
    ));

    #[cfg(unix)]
    assert_unix_approval_oracle(
        &fixture,
        &post,
        &post_before_ledger,
        post_before_revision,
        post_before_current_revision,
    );

    #[cfg(windows)]
    {
        assert_eq!(
            std::fs::read_to_string(&post.source).unwrap(),
            "pub const VALUE: i32 = 1;\n"
        );
        let ledger = runtime_ledger(&fixture);
        assert_eq!(
            event_delta(&post_before_ledger, &ledger, "session.resume.selected"),
            1
        );
    }

    let post_fault_ledger = runtime_ledger(&fixture);
    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    assert_eq!(
        runtime_ledger(&fixture),
        post_fault_ledger,
        "restart must not redispatch the committed intent"
    );
    terminal.send(&format!("select {}\n", post.workflow_id));
    terminal.wait_for(&format!("선택: {}", post.workflow_id));
    terminal.send("deny\n");
    terminal.wait_for("상태 변경을 확인하려면 yes를 입력하세요.");
    terminal.send("yes\n");
    #[cfg(unix)]
    {
        let denial_output = terminal.wait_for("다음: 롤백 영수증을 확인하세요.");
        native_terminal_denial_block_outcomes_exact(
            &denial_output,
            "deny.verification.rolled-back",
            &post.workflow_id,
            None,
        );
    }
    #[cfg(windows)]
    {
        let denial_output = terminal.wait_for("다음: 거부 영수증을 확인하세요.");
        native_terminal_denial_block_outcomes_exact(
            &denial_output,
            "deny.patch.accepted",
            &post.workflow_id,
            None,
        );
    }

    #[cfg(unix)]
    std::fs::write(&post.source, "pub const VALUE: i32 = 2;\n").unwrap();

    let resumable = fixture.prepare_source_approval();
    terminal.send(&format!("select {}\n", resumable.workflow_id));
    terminal.wait_for(&format!("선택: {}", resumable.workflow_id));
    terminal.send("resume\n");
    terminal.wait_for("상태 변경을 확인하려면 yes를 입력하세요.");
    terminal.send("yes\n");
    terminal.wait_for("resume.accepted");
    terminal.send("cancel\n");
    terminal.wait_for("상태 변경을 확인하려면 yes를 입력하세요.");
    terminal.send("yes\n");
    terminal.wait_for("cancel.accepted");
    terminal.send("view monitor\n");
    terminal.wait_for("rpotato | monitor");
    terminal.send("quit\n");
    let output = terminal.finish();
    std::env::remove_var("RPOTATO_TEST_TUI_SECRET_PROBE");
    assert!(output.contains("resume.accepted"));
    assert!(output.contains("cancel.accepted"));

    #[cfg(windows)]
    {
        let mut eof_terminal = NativePty::spawn(120, 40);
        eof_terminal.wait_for("›");
        eof_terminal.send_eof();
        let eof_output = eof_terminal.finish();
        assert!(
            !eof_output.contains("terminal.capability"),
            "the final EOF child must exit without a terminal capability failure"
        );
    }
}

fn assert_clean_restart() {
    let mut terminal = NativePty::spawn(120, 40);
    terminal.wait_for("›");
    terminal.send("quit\n");
    terminal.finish();
}

fn native_terminal_fault_outcomes_exact(fault: &str, capture: &str) {
    let capture = normalized_terminal_capture(capture);
    let expected = match fault {
        "invalid-fault-value" => "터미널 장애 주입 구성 오류\n- kind: InvalidFaultConfiguration\n- effect: NotDispatched\n- retry: FixConfiguration\n- 동작: 터미널 상태를 변경하거나 런타임 요청을 보내지 않았습니다.\n- 다음: RPOTATO_TEST_TERMINAL_FAULT 값을 닫힌 지원 목록으로 고치세요.".to_string(),
        "size-read" => "터미널 크기 확인 실패\n- code: terminal.capability.size-read\n- 동작: 런타임 요청을 보내지 않았습니다.\n- 다음: 읽기 전용 모드를 사용하세요.".to_string(),
        "mode-read" => "터미널 모드 확인 실패\n- code: terminal.capability.mode-read\n- 동작: 모드와 상태를 변경하지 않았습니다.\n- 다음: 터미널 모드를 확인한 뒤 다시 시도하세요.".to_string(),
        "no-echo-set" => "비밀 입력 보호 설정 실패\n- code: terminal.no-echo-set.failed\n- 동작: 비밀값을 읽거나 요청을 보내지 않았습니다.\n- 다음: 무반향 입력을 복구하세요.".to_string(),
        "secret-read" => "비밀 입력 읽기 실패\n- code: terminal.secret-read.failed\n- 동작: 비밀값을 수락하거나 저장하지 않았습니다.\n- 다음: 새 입력으로 다시 시도하세요.".to_string(),
        "frame-write-before-dispatch" => {
            let intent = capture_field(
                &capture,
                "terminal.frame-write.pre-dispatch",
                "intent",
            );
            format!(
                "요청 전 화면 출력 실패\n- code: terminal.frame-write.pre-dispatch\n- intent: {intent}\n- 동작: 런타임 요청을 보내지 않았습니다.\n- 다음: 정본 상태를 다시 읽고 요청하세요."
            )
        }
        "frame-write-after-dispatch" => {
            let intent = capture_field(
                &capture,
                "terminal.frame-write.post-dispatch",
                "intent",
            );
            format!(
                "커밋 후 화면 출력 실패\n- code: terminal.frame-write.post-dispatch\n- intent: {intent}\n- 동작: 요청을 다시 보내지 않습니다.\n- 다음: 영수증을 새로고침하세요."
            )
        }
        other => panic!("unknown native terminal fault oracle: {other}"),
    };
    assert_exact_outcome_block(&capture, &expected, fault);
}

fn native_terminal_denial_block_outcomes_exact(
    capture: &str,
    code: &str,
    workflow_id: &str,
    phase: Option<&str>,
) {
    let capture = normalized_terminal_capture(capture);
    let intent = capture_field(&capture, code, "intent");
    assert!(
        intent.starts_with("intent-tui-"),
        "native denial intent must be generated by the live controller: {intent}"
    );
    let expected = match code {
        "deny.patch.accepted" => format!(
            "패치 적용 거부 완료\n- code: deny.patch.accepted\n- intent: {intent}\n- workflow: {workflow_id}\n- 동작: 소스 변경 없이 취소 상태를 기록했습니다.\n- 다음: 거부 영수증을 확인하세요."
        ),
        "deny.verification.rolled-back" => format!(
            "검증 거부 및 롤백 완료\n- code: deny.verification.rolled-back\n- intent: {intent}\n- workflow: {workflow_id}\n- 동작: 원본 해시를 검증하고 취소 상태를 기록했습니다.\n- 다음: 롤백 영수증을 확인하세요."
        ),
        "deny.blocked.not-pending" => format!(
            "승인 대기 상태가 아니어서 거부 차단\n- code: deny.blocked.not-pending\n- intent: {intent}\n- workflow: {workflow_id}\n- phase: {}\n- 동작: 승인 상태와 효과를 변경하지 않았습니다.\n- 다음: 취소를 사용하거나 정본 상태를 새로고침하세요.",
            phase.expect("not-pending denial requires phase")
        ),
        "deny.blocked.terminal-state" => format!(
            "종료 상태여서 거부 차단\n- code: deny.blocked.terminal-state\n- intent: {intent}\n- workflow: {workflow_id}\n- phase: {}\n- 동작: 종료 상태와 영수증을 변경하지 않았습니다.\n- 다음: 기존 종료 영수증을 확인하세요.",
            phase.expect("terminal denial requires phase")
        ),
        other => panic!("unknown native denial oracle: {other}"),
    };
    assert_exact_outcome_block(&capture, &expected, code);
}

fn assert_exact_outcome_block(capture: &str, expected: &str, context: &str) {
    let expected_lines = expected.lines().collect::<Vec<_>>();
    assert!(expected_lines.len() >= 2, "invalid exact oracle: {context}");
    let marker = expected_lines[1];
    let capture_lines = capture.lines().collect::<Vec<_>>();
    let matches = capture_lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim_start() == marker)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "exact outcome marker count mismatch for {context}: {matches:?}\ncapture:\n{capture}"
    );
    let start = matches[0]
        .checked_sub(1)
        .unwrap_or_else(|| panic!("exact outcome header missing for {context}"));
    let end = start + expected_lines.len();
    assert!(
        end <= capture_lines.len(),
        "exact outcome truncated for {context}\ncapture:\n{capture}"
    );
    let mut actual_lines = capture_lines[start..end].to_vec();
    if actual_lines[0] != expected_lines[0] {
        actual_lines[0] = actual_lines[0]
            .strip_suffix(expected_lines[0])
            .map(|_| expected_lines[0])
            .unwrap_or(actual_lines[0]);
    }
    let actual = actual_lines.join("\n");
    assert_eq!(
        actual, expected,
        "exact outcome block mismatch for {context}\ncapture:\n{capture}"
    );
}

fn normalized_terminal_capture(capture: &str) -> String {
    native_terminal_support::strip_terminal_controls(capture)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| {
            line.strip_prefix("notice: ")
                .or_else(|| line.strip_prefix("        "))
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn normalizes_windows_title_and_cursor_control_sequences() {
    let capture = "\u{001b}[?25l결과\n- 다음: 완료.\u{001b}]0;C:\\rpotato.exe\u{0007}\u{001b}[?25h";
    assert_eq!(normalized_terminal_capture(capture), "결과\n- 다음: 완료.");
}

#[test]
fn conpty_mode_probe_parser_ignores_control_sequences_and_prompt_prefix() {
    let capture = b"\x1b[?9001h\xe2\x80\xba \x1b]0;C:\\rpotato.exe\x07MODE ECHO=1\x1b[?25h\r\n";
    assert_eq!(native_terminal_support::mode_probe_values(capture), ["1"]);
}

fn capture_field(capture: &str, code: &str, field: &str) -> String {
    let marker = format!("- code: {code}");
    let tail = capture
        .split_once(&marker)
        .unwrap_or_else(|| panic!("native capture missing exact code marker {marker}"))
        .1;
    let prefix = format!("- {field}: ");
    tail.lines()
        .find_map(|line| line.trim_start().strip_prefix(&prefix))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("native capture missing {field} after {code}"))
        .to_string()
}

fn assert_tree_unchanged(
    before: &std::collections::BTreeMap<String, Vec<u8>>,
    after: &std::collections::BTreeMap<String, Vec<u8>>,
    context: &str,
) {
    let changed = before
        .keys()
        .chain(after.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|path| !is_ignorable_entry_metadata(path) && before.get(*path) != after.get(*path))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        changed.is_empty(),
        "{context} must have zero product-state delta (coordination locks and the bounded latest-release cache excluded); changed paths: {changed:?}"
    );
}

fn is_ignorable_entry_metadata(path: &str) -> bool {
    path.ends_with(".lock") || path.replace('\\', "/").ends_with("/cache/update-latest-v2")
}

#[test]
fn zero_delta_entry_excludes_only_the_bounded_latest_release_cache() {
    assert!(is_ignorable_entry_metadata("1/cache/update-latest-v2"));
    assert!(is_ignorable_entry_metadata("1\\cache\\update-latest-v2"));
    assert!(!is_ignorable_entry_metadata(
        "1/cache/updates/v0.44.0/rpotato.ready"
    ));
    assert!(!is_ignorable_entry_metadata("1/state/current-state.json"));
}

fn runtime_ledger(fixture: &NativeTerminalFixture) -> String {
    std::fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap()
}

fn event_delta(before: &str, after: &str, event_type: &str) -> usize {
    let needle = format!("\"event_type\":\"{event_type}\"");
    after.matches(&needle).count() - before.matches(&needle).count()
}

#[cfg(unix)]
fn assert_unix_approval_oracle(
    fixture: &NativeTerminalFixture,
    pending: &native_terminal_support::PendingSourceApproval,
    before_ledger: &str,
    before_workflow_revision: u64,
    before_current_revision: u64,
) {
    assert_eq!(
        std::fs::read_to_string(&pending.source).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let ledger = runtime_ledger(fixture);
    let before_count = before_ledger.lines().count();
    let lines = ledger.lines().collect::<Vec<_>>();
    let committed = &lines[before_count..];
    let expected_types = [
        "runtime.intent.accepted",
        "workflow.checkpoint",
        "patch.apply.approved",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "hook.dispatched",
        "patch.applied",
        "transcript.recorded",
        "workflow.checkpoint",
    ];
    assert_eq!(committed.len(), expected_types.len(), "exact E0..E9 count");
    let mut ids = std::collections::BTreeSet::new();
    let mut previous = before_ledger
        .lines()
        .last()
        .map(|line| json_string(line, "event_hash"))
        .unwrap_or_else(|| "root".to_string());
    for (index, (line, expected_type)) in committed.iter().zip(expected_types).enumerate() {
        assert_eq!(
            json_string(line, "event_type"),
            expected_type,
            "E{index} event type"
        );
        assert_eq!(
            json_string(line, "previous_event_hash"),
            previous,
            "E{index} predecessor hash"
        );
        assert!(
            ids.insert(json_string(line, "event_id")),
            "E{index} event id must be unique"
        );
        previous = json_string(line, "event_hash");
    }
    for (event_type, expected) in [
        ("runtime.intent.accepted", 1),
        ("patch.apply.approved", 1),
        ("patch.applied", 1),
        ("transcript.recorded", 1),
        ("workflow.checkpoint", 2),
        ("hook.dispatched", 4),
    ] {
        assert_eq!(event_delta(before_ledger, &ledger, event_type), expected);
    }
    let pointer = std::fs::read_to_string(
        fixture
            .project
            .join(".rpotato/workflows")
            .join(format!("{}.json", pending.workflow_id)),
    )
    .unwrap();
    assert_eq!(
        json_u64(&pointer, "committed_revision"),
        before_workflow_revision + 2
    );
    let current =
        std::fs::read_to_string(fixture.project.join(".rpotato/state/current-state.json")).unwrap();
    assert_eq!(json_u64(&current, "revision"), before_current_revision + 1);
    assert_eq!(
        json_u64(&current, "event_count"),
        u64::try_from(before_count + 10).unwrap()
    );
    assert_eq!(
        json_string(&current, "event_id"),
        json_string(committed[9], "event_id")
    );
    assert_eq!(json_string(&current, "event_hash"), previous);
    let head =
        std::fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl.head")).unwrap();
    assert_eq!(
        json_u64(&head, "event_count"),
        u64::try_from(lines.len()).unwrap()
    );
    assert_eq!(json_string(&head, "last_event_hash"), previous);
    assert_eq!(
        std::fs::read_to_string(fixture.project.join(".rpotato/session-ledger.jsonl")).unwrap(),
        ledger,
        "T10 project ledger must exactly converge to runtime authority"
    );
    assert_eq!(
        std::fs::read(fixture.data.join("logs/operation.log")).unwrap(),
        expected_operation_log_bytes(&lines),
        "T10 operation log must exactly converge to runtime authority"
    );
    let projected = {
        let connection =
            rusqlite::Connection::open(fixture.data.join("state/observability.sqlite")).unwrap();
        let mut statement = connection
            .prepare(
                "SELECT rowid, event_id, ts_ms, event_type, project_id, session_id, summary
                   FROM ledger_events
               ORDER BY rowid",
            )
            .unwrap();
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        projected,
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                (
                    i64::try_from(index + 1).unwrap(),
                    json_string(line, "event_id"),
                    i64::try_from(json_u64(line, "ts_ms")).unwrap(),
                    json_string(line, "event_type"),
                    json_string(line, "project_id"),
                    json_string(line, "session_id"),
                    json_string(line, "summary"),
                )
            })
            .collect::<Vec<_>>(),
        "T10 sqlite rows and ordinals must exactly converge to runtime authority"
    );
    assert_directory_has_no_suffix(
        &fixture.project.join(".rpotato/transition-journal"),
        ".prepared.json",
    );
    assert_directory_has_no_suffix(&fixture.data.join("state/projection-lag"), ".json");
}

#[cfg(unix)]
fn expected_operation_log_bytes(lines: &[&str]) -> Vec<u8> {
    let mut output = lines
        .iter()
        .map(|line| {
            format!(
                "{} {} {} {}",
                json_u64(line, "ts_ms"),
                json_string(line, "event_type"),
                json_string(line, "session_id"),
                json_string(line, "summary")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    if !output.is_empty() {
        output.push(b'\n');
    }
    output
}

#[cfg(unix)]
fn assert_directory_has_no_suffix(path: &std::path::Path, suffix: &str) {
    if !path.exists() {
        return;
    }
    let mut pending = vec![path.to_path_buf()];
    let mut matches = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap().flatten() {
            if entry.path().is_dir() {
                pending.push(entry.path());
            } else if entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.ends_with(suffix))
            {
                matches.push(entry.path());
            }
        }
    }
    assert!(
        matches.is_empty(),
        "unexpected durable residue: {matches:?}"
    );
}

fn tree_contains(root: &std::path::Path, needle: &[u8]) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            tree_contains(&path, needle)
        } else {
            std::fs::read(path)
                .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
                .unwrap_or(false)
        }
    })
}

#[cfg(unix)]
fn json_u64(body: &str, key: &str) -> u64 {
    body.split(&format!("\"{key}\":"))
        .nth(1)
        .and_then(|tail| {
            let digits = tail
                .trim_start()
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>();
            digits.parse().ok()
        })
        .unwrap_or_else(|| panic!("missing numeric JSON field: {key}"))
}

#[cfg(unix)]
fn json_string(body: &str, key: &str) -> String {
    body.split(&format!("\"{key}\":"))
        .nth(1)
        .map(str::trim_start)
        .and_then(|tail| tail.strip_prefix('"'))
        .and_then(|tail| tail.split('"').next())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("missing string JSON field: {key}"))
        .to_string()
}
