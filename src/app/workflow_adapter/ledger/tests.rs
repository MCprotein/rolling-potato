use super::*;
use std::time::Duration;

#[test]
fn ledger_event_json_round_trip_for_projection_fields() {
    let event = LedgerEvent {
        event_id: "event-1".to_string(),
        ts_ms: 42,
        event_type: "runtime.init".to_string(),
        project_id: "project-a".to_string(),
        session_id: "session-a".to_string(),
        summary: "초기화".to_string(),
        details: "safe".to_string(),
    };

    let parsed = parse_event_line(&event.to_json_line()).unwrap();

    assert_eq!(parsed.event_id, "event-1");
    assert_eq!(parsed.ts_ms, 42);
    assert_eq!(parsed.event_type, "runtime.init");
    assert_eq!(parsed.project_id, "project-a");
    assert_eq!(parsed.session_id, "session-a");
    assert_eq!(parsed.summary, "초기화");
}

#[test]
fn redacts_sensitive_words_before_persistence() {
    let redacted = redact_text("token=abc safe password=hunter2");
    assert_eq!(redacted, "[REDACTED] safe [REDACTED]");
    assert!(contains_sensitive_text("Authorization: Bearer abc123"));
    assert!(contains_sensitive_text("key=sk-12345678"));
    assert!(!contains_sensitive_text("token budget 검증 완료"));
}

#[test]
fn malformed_runtime_ledger_line_fails_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root =
        std::env::temp_dir().join(format!("rpotato-ledger-malformed-{}", std::process::id()));
    std::env::set_var("RPOTATO_DATA_HOME", &root);
    fs::create_dir_all(paths::state_dir()).unwrap();
    fs::write(paths::runtime_ledger_file(), "{partial\n").unwrap();

    let error = read_runtime_events().unwrap_err();

    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
    assert_eq!(error.code, 3);
    assert!(error.message.contains("malformed JSONL"));
}

#[test]
fn read_only_tail_accepts_legacy_prefix_before_chained_suffix() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-ledger-read-only-legacy-prefix-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    fs::create_dir_all(paths::state_dir()).unwrap();
    let identity = fresh_identity();
    let path = paths::runtime_ledger_file();
    let mut legacy_prefix = String::new();
    for index in 0..62 {
        let event = new_event_for(
            &identity,
            "legacy.event",
            &format!("legacy {index}"),
            "safe",
        );
        legacy_prefix.push_str(&event.to_json_line());
        legacy_prefix.push('\n');
    }
    fs::write(&path, legacy_prefix).unwrap();
    for index in 0..61 {
        let event = new_event_for(
            &identity,
            "chained.event",
            &format!("chained {index}"),
            "safe",
        );
        storage::append_chained_event(&path, &event).unwrap();
    }

    let tail = read_runtime_tail_read_only(80, 2 * 1024 * 1024).unwrap();

    assert_eq!(tail.binding.event_count, 123);
    assert_eq!(tail.events.len(), 80);
    assert!(tail.truncated);
    assert_eq!(
        tail.events
            .iter()
            .filter(|event| event.event_hash.is_none())
            .count(),
        19
    );
    assert!(tail.events.last().unwrap().event_hash.is_some());

    let original = fs::read_to_string(&path).unwrap();
    let tampered = original.replacen("legacy 0", "legacy x", 1);
    assert_ne!(tampered, original);
    fs::write(&path, tampered).unwrap();
    let error = read_runtime_tail_read_only(80, 2 * 1024 * 1024).unwrap_err();
    assert!(error.message.contains("adjacent hash chain 불일치"));

    fs::write(&path, &original).unwrap();
    let incomplete_prefix_budget = u64::try_from(original.len() - 1).unwrap();
    let error = read_runtime_tail_read_only(80, incomplete_prefix_budget).unwrap_err();
    assert!(error
        .message
        .contains("legacy prefix가 read-only byte budget 안에 없습니다"));

    let first_chained_offset = original.find("{\"schema_version\":2").unwrap();
    let start_inside_last_legacy_record = first_chained_offset - 5;
    let chained_only_tail_budget =
        u64::try_from(original.len() - start_inside_last_legacy_record).unwrap();
    let error = read_runtime_tail_read_only(80, chained_only_tail_budget).unwrap_err();
    assert!(error
        .message
        .contains("legacy prefix가 read-only byte budget 안에 없습니다"));

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn workflow_checkpoint_previous_hash_chain_is_strict() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("rpotato-ledger-chain-{}", std::process::id()));
    std::env::set_var("RPOTATO_DATA_HOME", &root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    fs::create_dir_all(paths::project_state_dir()).unwrap();
    let identity = fresh_identity();
    let first_hash = "a".repeat(64);
    let second_hash = "b".repeat(64);
    let first = new_event_for(
        &identity,
        "workflow.checkpoint",
        "first",
        &format!(
            "workflow_id=workflow-chain revision=1 artifact_hash={first_hash} previous_hash=none phase=model-pending action_id=action proposal_id=none evidence_id=none"
        ),
    );
    let stale = new_event_for(
        &identity,
        "workflow.checkpoint",
        "stale",
        &format!(
            "workflow_id=workflow-chain revision=2 artifact_hash={second_hash} previous_hash={} phase=approved action_id=action proposal_id=none evidence_id=none",
            "c".repeat(64)
        ),
    );
    append_event(&first).unwrap();
    append_event(&stale).unwrap();

    let error = workflow_checkpoints("workflow-chain").unwrap_err();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(error.code, 3);
    assert!(error.message.contains("previous_hash chain"));
}

#[test]
fn physical_chain_reorder_and_truncation_fail_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for mode in ["reorder", "truncate"] {
        let root = std::env::temp_dir().join(format!(
            "rpotato-ledger-physical-{mode}-{}",
            std::process::id()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let identity = fresh_identity();
        append_event(&new_event_for(&identity, "one", "하나", "safe")).unwrap();
        append_event(&new_event_for(&identity, "two", "둘", "safe")).unwrap();
        let path = paths::runtime_ledger_file();
        let body = fs::read_to_string(&path).unwrap();
        let mut lines = body.lines().collect::<Vec<_>>();
        if mode == "reorder" {
            lines.swap(0, 1);
        } else {
            lines.pop();
        }
        fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
        assert!(read_runtime_events().is_err(), "mode: {mode}");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn runtime_head_repairs_only_the_single_durable_append_gap() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-ledger-head-repair-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let identity = fresh_identity();
    let first = new_event_for(&identity, "head.first", "첫 이벤트", "safe");
    append_event(&first).unwrap();
    let path = paths::runtime_ledger_file();
    let first_events = read_runtime_events().unwrap();
    let first_hash = first_events[0].event_hash.clone().unwrap();
    let second = new_event_for(&identity, "head.second", "두 번째 이벤트", "safe");
    let payload = event_chain_payload(&second, &first_hash);
    let second_hash = sha256_bytes(payload.as_bytes());
    let line = format!(
        "{{{},\"event_hash\":\"{}\"}}",
        payload.trim_start_matches('{').trim_end_matches('}'),
        second_hash
    );

    append_line(&path, &line).unwrap();
    let repaired = read_runtime_events().unwrap();
    let head = fs::read_to_string(ledger_head_path(&path)).unwrap();

    assert_eq!(repaired.len(), 2);
    assert!(head.contains("\"event_count\":2"));
    assert!(head.contains(&second_hash));

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_head_is_repaired_only_for_the_first_chained_append() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-ledger-first-head-repair-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let identity = fresh_identity();
    append_event(&new_event_for(&identity, "head.first", "첫 이벤트", "safe")).unwrap();
    let path = paths::runtime_ledger_file();
    fs::remove_file(ledger_head_path(&path)).unwrap();

    let repaired = read_runtime_events().unwrap();

    assert_eq!(repaired.len(), 1);
    assert!(ledger_head_path(&path).is_file());

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn orphan_runtime_head_without_jsonl_fails_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-ledger-orphan-head-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let path = paths::runtime_ledger_file();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_ledger_head(&path, 0, "root").unwrap();

    let error = read_runtime_events().unwrap_err();

    assert!(error.message.contains("orphan head"));
    assert!(!path.exists());
    assert!(ledger_head_path(&path).exists());
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn corrupt_project_mirror_is_preserved_and_rebuilt_from_runtime() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let identity = fresh_identity();
    let first = new_event_for(&identity, "mirror.first", "첫 이벤트", "safe");
    let second = new_event_for(&identity, "mirror.second", "두 번째 이벤트", "safe");
    append_event(&first).unwrap();

    let project_path = paths::project_session_ledger_file();
    let head_path = ledger_head_path(&project_path);
    fs::write(&project_path, "{malformed\n").unwrap();
    fs::write(&head_path, "{stale-head}\n").unwrap();

    append_event(&second).unwrap();

    let body = fs::read_to_string(&project_path).unwrap();
    let rebuilt = validate_ledger_contents(&project_path, &body).unwrap();
    let backups = fs::read_dir(paths::project_state_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".corrupt."))
        .collect::<Vec<_>>();
    let runtime = read_runtime_events().unwrap();

    assert_eq!(runtime.len(), 2);
    assert_eq!(rebuilt.len(), 2);
    assert_eq!(rebuilt[0].event_id, first.event_id);
    assert_eq!(rebuilt[1].event_id, second.event_id);
    assert_eq!(backups.len(), 2);
}

#[test]
fn concurrent_writers_preserve_both_ledger_chains() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-ledger-concurrent-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let identity = fresh_identity();
    let writers = 12;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(writers));
    let handles = (0..writers)
        .map(|index| {
            let barrier = barrier.clone();
            let identity = identity.clone();
            std::thread::spawn(move || {
                barrier.wait();
                append_event(&new_event_for(
                    &identity,
                    "concurrent.write",
                    &format!("writer {index}"),
                    "safe",
                ))
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().unwrap().unwrap();
    }
    let runtime_events = read_runtime_events().unwrap();
    let project_path = paths::project_session_ledger_file();
    let project_contents = fs::read_to_string(&project_path).unwrap();
    let project_events = validate_ledger_contents(&project_path, &project_contents).unwrap();
    let operation_log = fs::read_to_string(paths::operation_log_file()).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(runtime_events.len(), writers);
    assert_eq!(project_events.len(), writers);
    assert_eq!(operation_log.lines().count(), writers);
}

#[test]
fn event_sink_single_acquisition_concurrency_matrix() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-ledger-event-sink-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let identity = fresh_identity();
    let events = [
        new_event_for(&identity, "sink.first", "첫 이벤트", "index=0"),
        new_event_for(&identity, "sink.second", "두 번째 이벤트", "index=1"),
    ];
    let writer = LedgerWriterGuard::acquire().unwrap();
    let planned = writer.plan_events(&events).unwrap();
    let mut sink = writer.event_sink(&planned);
    let concurrent_identity = identity.clone();
    let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
    let (sender, receiver) = std::sync::mpsc::channel();
    let concurrent = std::thread::spawn(move || {
        let event = new_event_for(
            &concurrent_identity,
            "sink.concurrent",
            "경쟁 이벤트",
            "index=2",
        );
        let result = LedgerWriterGuard::acquire_after_first_block(|| {
            ready_sender.send(()).unwrap();
        })
        .and_then(|writer| writer.append_planned(&event).map(|_| ()));
        sender.send(result).unwrap();
    });
    ready_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("contender가 held lease에서 실제로 차단되어야 합니다.");
    assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());

    assert!(sink.append_planned_under_guard(1, &events[1]).is_err());
    sink.append_planned_under_guard(0, &events[0]).unwrap();
    assert!(sink.append_planned_under_guard(1, &events[0]).is_err());
    sink.append_planned_under_guard(1, &events[1]).unwrap();
    sink.finish().unwrap();
    sink.converge_derived(&identity.project_id).unwrap();
    drop(writer);
    receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    concurrent.join().unwrap();

    let runtime = read_runtime_events().unwrap();
    assert_eq!(runtime.len(), 3);
    assert_eq!(runtime[0].event_id, events[0].event_id);
    assert_eq!(runtime[1].event_id, events[1].event_id);
    assert_eq!(runtime[2].event_type, "sink.concurrent");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn event_sink_crash_recovery_never_nests_ledger_lease() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-ledger-event-sink-restart-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let identity = fresh_identity();
    let first = new_event_for(&identity, "sink.restart.first", "첫 이벤트", "index=0");
    let second = new_event_for(&identity, "sink.restart.second", "둘째 이벤트", "index=1");
    {
        let writer = LedgerWriterGuard::acquire().unwrap();
        let planned = writer.plan_events(std::slice::from_ref(&first)).unwrap();
        let mut sink = writer.event_sink(&planned);
        sink.append_planned_under_guard(0, &first).unwrap();
    }
    {
        let writer = LedgerWriterGuard::acquire().unwrap();
        let planned = writer.plan_events(std::slice::from_ref(&second)).unwrap();
        let mut sink = writer.event_sink(&planned);
        sink.append_planned_under_guard(0, &second).unwrap();
        sink.finish().unwrap();
        sink.converge_derived(&identity.project_id).unwrap();
    }
    let events = read_runtime_events().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_id, first.event_id);
    assert_eq!(events[1].event_id, second.event_id);
    let source = include_str!("writer.rs")
        .split("impl EventSink<'_> {")
        .nth(1)
        .unwrap()
        .split("fn validate_prepared_runtime_suffix")
        .next()
        .unwrap();
    assert!(!source.contains("LedgerWriterGuard::acquire"));
    assert!(!source.contains("RecoverableLease::acquire"));
    assert!(!source.contains("append_event("));
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn t10_rebuilds_all_derived_outputs_from_runtime_authority() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-ledger-t10-convergence-{}-{}",
        std::process::id(),
        now_nanos()
    ));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    fs::create_dir_all(paths::project_root()).unwrap();
    let identity = fresh_identity();
    let first = new_event_for(&identity, "t10.first", "첫 이벤트", "safe=one");
    let second = new_event_for(&identity, "t10.second", "두 번째 이벤트", "safe=two");
    append_event(&first).unwrap();
    append_event(&second).unwrap();
    crate::app::observability_adapter::converge_from_events(&read_runtime_events().unwrap())
        .unwrap();

    let project_path = paths::project_session_ledger_file();
    {
        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
        connection
            .execute(
                "UPDATE ledger_events SET summary = 'tampered-same-id' WHERE event_id = ?1",
                rusqlite::params![first.event_id],
            )
            .unwrap();
    }
    assert!(validate_derived_outputs_unlocked(
        &read_runtime_events().unwrap(),
        &identity.project_id
    )
    .unwrap_err()
    .message
    .contains("sqlite convergence event sequence"));
    fs::write(&project_path, b"{corrupt-project-ledger\n").unwrap();
    fs::write(ledger_head_path(&project_path), b"{corrupt-head}\n").unwrap();
    fs::write(paths::operation_log_file(), b"stale extra operation\n").unwrap();

    let writer = LedgerWriterGuard::acquire().unwrap();
    writer.converge_derived(&identity.project_id).unwrap();
    let runtime = writer.events().unwrap();
    crate::app::observability_adapter::converge_from_events(&runtime).unwrap();
    drop(writer);

    let project_events = runtime
        .iter()
        .filter(|event| event.project_id == identity.project_id)
        .cloned()
        .collect::<Vec<_>>();
    let (expected_project, expected_head_hash) = render_chained_ledger(&project_events);
    let expected_head = format!(
        "{{\"schema_version\":1,\"event_count\":{},\"last_event_hash\":\"{}\"}}\n",
        project_events.len(),
        expected_head_hash.as_deref().unwrap_or("root")
    );
    let expected_operation_log = runtime
        .iter()
        .map(|event| {
            format!(
                "{} {} {} {}\n",
                event.ts_ms, event.event_type, event.session_id, event.summary
            )
        })
        .collect::<String>();

    assert_eq!(fs::read_to_string(&project_path).unwrap(), expected_project);
    assert_eq!(
        fs::read_to_string(ledger_head_path(&project_path)).unwrap(),
        expected_head
    );
    assert_eq!(
        fs::read_to_string(paths::operation_log_file()).unwrap(),
        expected_operation_log
    );
    let projected_rows = {
        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
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
        projected_rows,
        runtime
            .iter()
            .enumerate()
            .map(|(index, event)| (
                i64::try_from(index + 1).unwrap(),
                event.event_id.clone(),
                i64::try_from(event.ts_ms).unwrap(),
                event.event_type.clone(),
                event.project_id.clone(),
                event.session_id.clone(),
                event.summary.clone(),
            ))
            .collect::<Vec<_>>()
    );

    let before_restart = (
        fs::read(&project_path).unwrap(),
        fs::read(ledger_head_path(&project_path)).unwrap(),
        fs::read(paths::operation_log_file()).unwrap(),
    );
    let writer = LedgerWriterGuard::acquire().unwrap();
    writer.converge_derived(&identity.project_id).unwrap();
    crate::app::observability_adapter::converge_from_events(&writer.events().unwrap()).unwrap();
    drop(writer);
    assert_eq!(
        before_restart,
        (
            fs::read(&project_path).unwrap(),
            fs::read(ledger_head_path(&project_path)).unwrap(),
            fs::read(paths::operation_log_file()).unwrap(),
        )
    );

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}
