use super::*;

#[test]
fn fixture_retries_backend_start_after_ephemeral_port_collision() {
    let fixture = fixture("backend-port-retry");
    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();
    fixture.port.store(occupied_port, Ordering::Relaxed);

    fixture.start();

    assert_ne!(fixture.port.load(Ordering::Relaxed), occupied_port);
}

#[test]
fn happy_path_is_restart_safe_and_reports_korean() {
    let fixture = fixture("happy-subprocess");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_out = String::from_utf8(run.stdout).unwrap();
    assert!(!run_out.contains("MODEL ACTION"));
    assert!(!run_out.contains("- response:"));
    assert!(run_out.contains("raw response는 표시하지 않음"));
    let proposal = field(&run_out, "proposal id");
    let token = run_out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let resume = fixture.command(&["state", "resume"]);
    assert!(resume.status.success());
    let resume_out = String::from_utf8_lossy(&resume.stdout);
    assert!(resume_out.contains("backend 호출: 없음"));
    assert!(resume_out.contains("token 재표시: 불가"));
    assert!(!resume_out.contains(&token));
    let tui = fixture.command(&["tui", "diff", &proposal]);
    assert!(tui.status.success());
    assert!(!String::from_utf8_lossy(&tui.stdout).contains(&token));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );

    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(
        approve.status.success(),
        "{}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    assert!(approve_report.starts_with("patch approve\n- status: applied-awaiting-verification"));
    assert!(approve_report.contains("verification approval: required"));
    assert!(approve_report.contains("verification command는 아직 실행하지 않았습니다"));
    assert!(!approve_report.contains("패치 작업 완료"));
    assert!(!approve_report.contains("MODEL ACTION"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let verify_token = verification_token(&approve_report);

    let resumed = fixture.command(&["state", "resume"]);
    assert!(resumed.status.success());
    let resumed_out = String::from_utf8_lossy(&resumed.stdout);
    assert!(resumed_out.contains("verification 승인 대기"));
    assert!(resumed_out.contains("verification 실행: 없음"));
    assert!(!resumed_out.contains(&verify_token));

    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let report = String::from_utf8(verify.stdout).unwrap();
    assert!(report.starts_with("패치 작업 완료\n- 결과: 성공"));
    assert!(report.contains("stop gate: 통과"));
    assert!(!report.contains("MODEL ACTION"));

    let ledger_path = fixture.data.join("state/runtime-ledger.jsonl");
    let event_count = fs::read_to_string(&ledger_path).unwrap().lines().count();
    let repeated = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        repeated.status.success(),
        "status={:?}\nstderr={}\nstdout={}",
        repeated.status.code(),
        String::from_utf8_lossy(&repeated.stderr),
        String::from_utf8_lossy(&repeated.stdout)
    );
    assert_eq!(
        fs::read_to_string(&ledger_path).unwrap().lines().count(),
        event_count,
        "complete resume must not duplicate ledger events"
    );
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );
}

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

#[test]
fn durable_transcript_rebuilds_after_db_loss_and_continue_is_idempotent() {
    let fixture = fixture("durable-conversation-resume");
    fixture.start();

    let run = fixture.command(&["run", "src/lib.rs의 값을 2로 고쳐줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let list = fixture.command(&["session", "list"]);
    assert!(list.status.success());
    let session_id = String::from_utf8(list.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("- current session: "))
        .unwrap()
        .to_string();

    let transcript = fixture.command(&["tui", "transcript", &session_id]);
    assert!(transcript.status.success());
    let transcript_report = String::from_utf8(transcript.stdout).unwrap();
    assert!(transcript_report.contains("[durable conversation]"));
    assert!(transcript_report.contains("user |"));
    assert!(transcript_report.contains("tool |"));
    assert!(transcript_report.contains("model |"));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );

    let db = fixture.data.join("state/observability.sqlite");
    let _ = fs::remove_file(&db);
    let _ = fs::remove_file(db.with_extension("sqlite-wal"));
    let _ = fs::remove_file(db.with_extension("sqlite-shm"));

    for args in [vec!["continue"], vec!["resume", session_id.as_str()]] {
        let resumed = fixture.command(&args);
        assert!(
            resumed.status.success(),
            "{}",
            String::from_utf8_lossy(&resumed.stderr)
        );
        let report = String::from_utf8(resumed.stdout).unwrap();
        assert!(
            report.contains("reconstructed context: context limit=1024 transcript turns=3"),
            "{report}"
        );
        assert!(report.contains("backend 호출: 없음"));
        assert_eq!(
            fs::read_to_string(&fixture.calls).unwrap().lines().count(),
            1
        );
    }

    let status = fixture.command(&["state"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(String::from_utf8(status.stdout)
        .unwrap()
        .contains("transcript records: 3"));

    let project_transcripts = fixture.data.join("state/transcripts");
    let project_dir = fs::read_dir(project_transcripts)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let session_dir = project_dir.join(&session_id);
    let artifact = fs::read_dir(session_dir)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .expect("canonical transcript JSON artifact");
    fs::write(artifact, "{}\n").unwrap();

    let blocked = fixture.command(&["continue"]);
    assert_eq!(blocked.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );
}

#[test]
fn patch_transcript_excludes_source_fragments_from_durable_surfaces() {
    const SECRET: &str = "RPOTATO_SECRET_SOURCE_FRAGMENT";
    let fixture = fixture("transcript-source-redaction");
    fs::write(
        fixture.project.join("src/lib.rs"),
        format!("pub const VALUE: &str = \"{SECRET}\";\n"),
    )
    .unwrap();
    let find_hex = SECRET
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    fs::write(
        &fixture.response,
        format!(
            "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex={find_hex}; replace_hex=7265646163746564; verification=pwd; next_gate=diff-before-write; side_effects=none"
        ),
    )
    .unwrap();
    fixture.start();

    let run = fixture.command(&["run", "상수 값을 안전한 값으로 바꿔줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let list = fixture.command(&["session", "list"]);
    let session_id = String::from_utf8(list.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("- current session: "))
        .unwrap()
        .to_string();
    let tui = fixture.command(&["tui", "transcript", &session_id]);
    assert!(tui.status.success());
    assert!(!String::from_utf8_lossy(&tui.stdout).contains(SECRET));

    for path in [
        fixture.data.join("state/transcripts"),
        fixture.data.join("state/runtime-ledger.jsonl"),
        fixture.data.join("state/observability.sqlite"),
    ] {
        assert!(
            !path_contains_bytes(&path, SECRET.as_bytes()),
            "secret leaked into {}",
            path.display()
        );
    }
}

#[test]
fn read_only_run_completes_without_patch_gate() {
    let fixture = fixture("read-only-subprocess");
    fs::write(
        &fixture.response,
        "src/lib.rs 구조를 확인했으며 파일 변경은 필요하지 않습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();

    let run = fixture.command(&["run", "저장소 구조를 분석해줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    assert!(report.starts_with("run 결과\n- 상태: 완료"));
    assert!(report.contains("- action kind: inspect-sources"));
    assert!(report.contains("- side effect: 없음"));
    assert!(report.contains("src/lib.rs 구조를 확인했으며 파일 변경은 필요하지 않습니다."));
    assert!(!report.contains("MODEL ACTION"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let workflow_id = field(&report, "workflow id");
    let snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{workflow_id}.snapshots"));
    let latest = fs::read_dir(snapshots)
        .unwrap()
        .filter_map(Result::ok)
        .max_by_key(|entry| entry.file_name())
        .unwrap();
    let stored = fs::read_to_string(latest.path()).unwrap();
    assert!(stored.contains("\"workflow_kind\": \"agent-run\""));
    assert!(stored.contains("\"action_kind\": \"inspect-sources\""));
    assert!(stored.contains("\"phase\": \"complete\""));

    let status = fixture.command(&["state"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(String::from_utf8_lossy(&status.stdout).contains("active workflow: 없음"));
}

#[test]
fn imported_codex_skill_runs_through_read_only_runtime_boundaries() {
    let fixture = fixture("imported-codex-skill");
    let plugin = fixture.root.join("safe-plugin");
    fs::create_dir_all(plugin.join(".codex-plugin")).unwrap();
    fs::create_dir_all(plugin.join("skills/hello")).unwrap();
    fs::write(
        plugin.join(".codex-plugin/plugin.json"),
        r#"{"name":"safe-plugin","version":"1.0.0","description":"safe"}"#,
    )
    .unwrap();
    fs::write(
        plugin.join("skills/hello/SKILL.md"),
        "---\nname: hello\ndescription: 저장소를 읽기 전용으로 설명한다.\n---\n근거 파일을 확인하고 한국어로 설명하세요.\n",
    )
    .unwrap();
    fs::write(
        &fixture.response,
        "src/lib.rs를 읽기 전용으로 확인했으며 파일은 변경하지 않았습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();

    for args in [
        vec![
            "plugin",
            "import",
            "--from",
            "codex",
            plugin.to_str().unwrap(),
        ],
        vec!["plugin", "validate", "imported.codex.safe-plugin"],
        vec!["plugin", "enable", "imported.codex.safe-plugin"],
    ] {
        let output = fixture.command(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let run = fixture.command(&[
        "skill",
        "run",
        "imported.codex.safe-plugin.hello",
        "현재 저장소를 설명해줘",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    assert!(report.contains("- plugin boundary: instruction-only/read-only"));
    assert!(report.contains("- plugin source: skills/hello/SKILL.md@"));
    assert!(report.contains("src/lib.rs를 읽기 전용으로 확인"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let workflow_id = field(&report, "workflow id");
    let snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{workflow_id}.snapshots"));
    let latest = fs::read_dir(snapshots)
        .unwrap()
        .filter_map(Result::ok)
        .max_by_key(|entry| entry.file_name())
        .unwrap();
    let stored = fs::read_to_string(latest.path()).unwrap();
    assert!(stored.contains("\"workflow_kind\": \"plugin-capability\""));
    assert!(stored.contains("\"source_path\": \"skills/hello/SKILL.md\""));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("plugin.capability.admitted"));
    assert!(ledger.contains("plugin.capability.completed"));
}

#[test]
fn imported_claude_command_runs_through_read_only_runtime_boundaries() {
    let fixture = fixture("imported-claude-command");
    let plugin = fixture.root.join("safe-claude-plugin");
    fs::create_dir_all(plugin.join(".claude-plugin")).unwrap();
    fs::create_dir_all(plugin.join("commands")).unwrap();
    fs::write(
        plugin.join(".claude-plugin/plugin.json"),
        r#"{"name":"safe-claude-plugin","version":"1.0.0","description":"safe"}"#,
    )
    .unwrap();
    fs::write(
        plugin.join("commands/explain.md"),
        "---\ndescription: 저장소를 읽기 전용으로 설명한다.\n---\n근거 파일을 확인하고 한국어로 설명하세요.\n",
    )
    .unwrap();
    fs::write(
        &fixture.response,
        "src/lib.rs를 읽기 전용으로 확인했으며 파일은 변경하지 않았습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();

    for args in [
        vec![
            "plugin",
            "import",
            "--from",
            "claude-code",
            plugin.to_str().unwrap(),
        ],
        vec![
            "plugin",
            "validate",
            "imported.claude-code.safe-claude-plugin",
        ],
        vec![
            "plugin",
            "enable",
            "imported.claude-code.safe-claude-plugin",
        ],
    ] {
        let output = fixture.command(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let run = fixture.command(&[
        "skill",
        "run",
        "imported.claude-code.safe-claude-plugin.explain",
        "현재 저장소를 설명해줘",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    assert!(report.contains("- plugin boundary: instruction-only/read-only"));
    assert!(report.contains("- plugin source: commands/explain.md@"));
    assert!(report.contains("src/lib.rs를 읽기 전용으로 확인"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let workflow_id = field(&report, "workflow id");
    let snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{workflow_id}.snapshots"));
    let latest = fs::read_dir(snapshots)
        .unwrap()
        .filter_map(Result::ok)
        .max_by_key(|entry| entry.file_name())
        .unwrap();
    let stored = fs::read_to_string(latest.path()).unwrap();
    assert!(stored.contains("\"workflow_kind\": \"plugin-capability\""));
    assert!(stored.contains("\"source_path\": \"commands/explain.md\""));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("plugin.capability.admitted"));
    assert!(ledger.contains("plugin.capability.completed"));
}

#[test]
fn imported_codex_skill_completion_recovery_is_idempotent() {
    for (fault, expected_before) in [("before-event", 0), ("before-pointer-clear", 1)] {
        let fixture = fixture(&format!("imported-codex-recovery-{fault}"));
        let plugin = fixture.root.join("safe-plugin");
        fs::create_dir_all(plugin.join(".codex-plugin")).unwrap();
        fs::create_dir_all(plugin.join("skills/hello")).unwrap();
        fs::write(
            plugin.join(".codex-plugin/plugin.json"),
            r#"{"name":"safe-plugin","version":"1.0.0","description":"safe"}"#,
        )
        .unwrap();
        fs::write(
            plugin.join("skills/hello/SKILL.md"),
            "---\nname: hello\ndescription: 저장소를 읽기 전용으로 설명한다.\n---\n근거 파일을 확인하고 한국어로 설명하세요.\n",
        )
        .unwrap();
        fs::write(
            &fixture.response,
            "src/lib.rs를 읽기 전용으로 확인했으며 파일은 변경하지 않았습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
        )
        .unwrap();
        fixture.start();

        for args in [
            vec![
                "plugin",
                "import",
                "--from",
                "codex",
                plugin.to_str().unwrap(),
            ],
            vec!["plugin", "validate", "imported.codex.safe-plugin"],
            vec!["plugin", "enable", "imported.codex.safe-plugin"],
        ] {
            let output = fixture.command(&args);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let args = [
            "skill",
            "run",
            "imported.codex.safe-plugin.hello",
            "현재 저장소를 설명해줘",
        ];
        let mut command = fixture.command_builder(&args);
        command.env("RPOTATO_TEST_PLUGIN_COMPLETION_FAULT", fault);
        let child = spawn_captured(&mut command).unwrap();
        let interrupted = wait_bounded(child, &args);
        assert!(!interrupted.status.success());
        let interrupted_error = String::from_utf8_lossy(&interrupted.stderr);
        assert!(!interrupted_error.is_empty(), "missing error for {fault}");

        let ledger_path = fixture.data.join("state/runtime-ledger.jsonl");
        let before = fs::read_to_string(&ledger_path).unwrap();
        assert_eq!(
            before.matches("plugin.capability.completed").count(),
            expected_before
        );

        let resume = fixture.command(&["state", "resume"]);
        assert!(
            resume.status.success(),
            "{}",
            String::from_utf8_lossy(&resume.stderr)
        );
        assert!(String::from_utf8_lossy(&resume.stdout).contains("plugin capability 복구 완료"));
        let after = fs::read_to_string(&ledger_path).unwrap();
        assert_eq!(after.matches("plugin.capability.completed").count(), 1);

        let status = fixture.command(&["state"]);
        assert!(status.status.success());
        assert!(String::from_utf8_lossy(&status.stdout).contains("active workflow: 없음"));
    }
}
