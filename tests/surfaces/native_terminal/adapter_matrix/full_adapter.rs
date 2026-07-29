use super::*;

#[cfg(any(target_os = "linux", target_os = "macos", windows))]
#[test]
fn full_adapter() {
    trace_stage("full_adapter start");
    let fixture = NativeTerminalFixture::new("full-adapter");
    trace_stage("fixture initialized");
    let pending = fixture.prepare_source_approval();
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
        select_workflow(&mut terminal, &pending.workflow_id);
        submit_visible_command(&mut terminal, &format!("approve {}", pending.proposal_id));
        confirm_picker(&mut terminal, "패치 적용 확인");
    }
    #[cfg(windows)]
    {
        select_workflow(&mut terminal, &pending.workflow_id);
        submit_visible_command(&mut terminal, "deny");
        confirm_picker(&mut terminal, "요청 거부 확인");
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
    select_workflow(&mut terminal, &pending.workflow_id);
    submit_visible_command(&mut terminal, &format!("approve {}", pending.proposal_id));
    #[cfg(unix)]
    {
        confirm_picker(&mut terminal, "패치 적용 확인");
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
        submit_visible_command(&mut terminal, "deny");
        confirm_picker(&mut terminal, "요청 거부 확인");
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
        submit_visible_command(&mut terminal, "deny");
        confirm_picker(&mut terminal, "요청 거부 확인");
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
        submit_visible_command(&mut terminal, "deny");
        confirm_picker(&mut terminal, "요청 거부 확인");
        let denial_output = terminal.wait_for("다음: 거부 영수증을 확인하세요.");
        native_terminal_denial_block_outcomes_exact(
            &denial_output,
            "deny.patch.accepted",
            &pending.workflow_id,
            None,
        );
        submit_visible_command(&mut terminal, "deny");
        confirm_picker(&mut terminal, "요청 거부 확인");
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
    select_workflow(&mut terminal, &post.workflow_id);
    submit_visible_command(&mut terminal, &format!("approve {}", post.proposal_id));
    #[cfg(unix)]
    {
        confirm_picker(&mut terminal, "패치 적용 확인");
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
        submit_visible_command(&mut terminal, "deny");
        confirm_picker(&mut terminal, "요청 거부 확인");
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
            event_delta(&post_before_ledger, &ledger, "patch.apply.denied"),
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
    select_workflow(&mut terminal, &post.workflow_id);
    submit_visible_command(&mut terminal, "deny");
    confirm_picker(&mut terminal, "요청 거부 확인");
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
        let denial_output = terminal.wait_for("다음: 기존 종료 영수증을 확인하세요.");
        native_terminal_denial_block_outcomes_exact(
            &denial_output,
            "deny.blocked.terminal-state",
            &post.workflow_id,
            Some("cancelled"),
        );
    }

    #[cfg(unix)]
    std::fs::write(&post.source, "pub const VALUE: i32 = 2;\n").unwrap();

    let resumable = fixture.prepare_source_approval();
    select_workflow(&mut terminal, &resumable.workflow_id);
    submit_visible_command(&mut terminal, "resume");
    confirm_picker(&mut terminal, "작업 재개 확인");
    terminal.wait_for("resume.accepted");
    submit_visible_command(&mut terminal, "cancel");
    confirm_picker(&mut terminal, "작업 취소 확인");
    terminal.wait_for("cancel.accepted");
    submit_visible_command(&mut terminal, "view monitor");
    terminal.wait_for("rpotato | monitor");
    submit_visible_command(&mut terminal, "quit");
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
