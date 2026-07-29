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
