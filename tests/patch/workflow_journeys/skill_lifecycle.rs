#[test]
fn explicit_skill_run_persists_lifecycle_state_and_sqlite_projection() {
    let fixture = fixture("explicit-skill-lifecycle");
    fixture.start();

    let run = fixture.command(&[
        "skill",
        "run",
        "small-patch",
        "src/lib.rs의 값을 2로 고쳐줘",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    assert!(report.contains("- invocation: explicit-skill"));
    assert!(report.contains("- selected skill: small-patch"));
    let workflow_id = field(&report, "workflow id");
    let proposal = field(&report, "proposal id");
    let token = command_token(&report, "- approval command: rpotato patch approve ");

    let pending = latest_workflow_snapshot(&fixture, &workflow_id);
    assert!(pending.contains("\"active_skill_id\": \"small-patch\""));
    assert!(pending.contains("\"skill_invocation\": \"explicit\""));
    assert!(pending.contains("\"skill_state\": \"awaiting-approval\""));
    assert!(pending.contains("pre_model_request"));
    assert!(pending.contains("diff_review"));

    let connection =
        rusqlite::Connection::open(fixture.data.join("state/observability.sqlite")).unwrap();
    let projected_skill: String = connection
        .query_row(
            "SELECT active_skill_id FROM workflows WHERE workflow_id = ?1",
            [&workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(projected_skill, "small-patch");

    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(approve.status.success());
    let verify_token = verification_token(&String::from_utf8(approve.stdout).unwrap());
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );

    let complete = latest_workflow_snapshot(&fixture, &workflow_id);
    assert!(complete.contains("\"skill_state\": \"complete\""));
    assert!(complete.contains("diff_review,targeted_verification"));
    assert!(complete.contains("patch_applied,verification_passed,korean_report_passed"));
    for hook in [
        "session_start",
        "user_request_received",
        "pre_context_pack",
        "post_context_pack",
        "pre_model_request",
        "post_model_response",
        "pre_action_parse",
        "post_action_parse",
        "pre_tool_call",
        "post_tool_result",
        "pre_patch_apply",
        "post_patch_apply",
        "pre_command_run",
        "post_command_run",
        "pre_final_report",
        "stop_gate",
        "session_end",
    ] {
        assert!(complete.contains(hook), "missing persisted hook: {hook}");
    }
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("\"event_type\":\"hook.dispatched\""));
    assert!(ledger.contains("hook=session_start"));
    assert!(ledger.contains("hook=stop_gate"));
}

#[test]
fn explicit_skill_missing_context_fails_before_model_call() {
    let fixture = fixture("explicit-skill-missing-context");
    fixture.start();

    let run = fixture.command(&["skill", "run", "fix-test", "실패한 테스트를 고쳐줘"]);

    assert_eq!(run.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("skill requirement 차단"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!fixture.calls.exists());
    let state = fixture.command(&["state"]);
    assert!(state.status.success());
    assert!(String::from_utf8_lossy(&state.stdout).contains("active workflow: 없음"));
}

#[test]
fn fix_test_records_real_failure_before_patch_and_pass_after_patch() {
    let fixture = fixture("fix-test-real-evidence");
    setup_failing_test_project(&fixture);
    fs::write(
        &fixture.response,
        "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=cargo test; next_gate=diff-before-write; side_effects=none",
    )
    .unwrap();
    fixture.start();

    let run = fixture.command(&[
        "skill",
        "run",
        "fix-test",
        "src/lib.rs 테스트 결과: test result: FAILED, VALUE는 2여야 합니다.",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    let workflow_id = field(&report, "workflow id");
    let proposal = field(&report, "proposal id");
    let token = command_token(&report, "- approval command: rpotato patch approve ");
    let pending = latest_workflow_snapshot(&fixture, &workflow_id);
    assert!(pending.contains("failing_test_before"));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("\"event_type\":\"skill.test_failure.observed\""));
    assert!(ledger.contains(&format!("workflow_id={workflow_id}")));

    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(
        approve.status.success(),
        "{}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let verify_token = verification_token(&String::from_utf8(approve.stdout).unwrap());
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );

    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let complete = latest_workflow_snapshot(&fixture, &workflow_id);
    assert!(complete.contains("\"skill_state\": \"complete\""));
    assert!(complete.contains("failing_test_before,passing_test_after"));
}

#[test]
fn fix_test_rejects_non_test_verification_without_leaving_active_workflow() {
    let fixture = fixture("fix-test-non-test-verification");
    setup_failing_test_project(&fixture);
    fixture.start();

    let run = fixture.command(&[
        "skill",
        "run",
        "fix-test",
        "src/lib.rs 테스트 결과: test result: FAILED, VALUE는 2여야 합니다.",
    ]);

    assert_eq!(run.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&run.stderr).contains("cargo test"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );
    let state = fixture.command(&["state"]);
    assert!(state.status.success());
    assert!(String::from_utf8_lossy(&state.stdout).contains("active workflow: 없음"));
}

#[test]
fn read_only_action_without_visible_answer_fails_closed() {
    let fixture = fixture("read-only-empty-answer");
    fs::write(
        &fixture.response,
        "MODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();

    let run = fixture.command(&["skill", "run", "repo-map", "저장소 구조를 분석해줘"]);

    assert_eq!(run.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&run.stderr).contains("답변이 비어 있습니다"));
    let state = fixture.command(&["state"]);
    assert!(state.status.success());
    assert!(String::from_utf8_lossy(&state.stdout).contains("active workflow: 없음"));
}

fn latest_workflow_snapshot(fixture: &Fixture, workflow_id: &str) -> String {
    let snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{workflow_id}.snapshots"));
    let latest = fs::read_dir(snapshots)
        .unwrap()
        .filter_map(Result::ok)
        .max_by_key(|entry| entry.file_name())
        .unwrap();
    fs::read_to_string(latest.path()).unwrap()
}
