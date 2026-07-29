#[test]
fn one_shot_outcome_writes_secret_once_without_storing_it_in_notice() {
    let intent_id = "intent-one-shot-test";
    let secret = "ab".repeat(32);
    let outcome =
        verification_credential_issued(intent_id, OneShotSecret::new(secret.clone()).unwrap())
            .unwrap();
    let mut terminal = ScriptedTerminal::new([]);

    let notice = consume_outcome(&mut terminal, intent_id, outcome).unwrap();

    assert_eq!(terminal.frames.len(), 3);
    let rendered = terminal.frames.concat();
    assert_eq!(
        rendered.matches(&secret).count(),
        1,
        "credential must be written exactly once"
    );
    assert!(notice.was_dispatched);
    assert!(!notice.notice.contains(&secret));
    assert!(notice.notice.contains("verification.credential-issued"));
}

#[test]
fn ordinary_line_read_failure_has_a_distinct_non_secret_taxonomy() {
    let error = terminal_fault_error(TerminalFault::LineRead);

    assert!(error.message.contains("terminal.capability.mode-read"));
    assert!(!error.message.contains("terminal.secret-read.failed"));
}

#[test]
fn echo_restore_failure_exits_without_retrying_secret_input() {
    let error = terminal_fault_error(TerminalFault::EchoRestore);

    assert!(error.message.contains("terminal.echo-restore.failed"));
    assert!(error.message.contains("재시도하지 않고 TUI를 종료"));
    assert!(error.message.contains("stty echo"));
    assert!(!error.message.contains("terminal.secret-read.failed"));
}
