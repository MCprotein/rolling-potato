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
