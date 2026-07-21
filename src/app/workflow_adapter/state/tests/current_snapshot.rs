use super::*;

#[test]
fn current_state_summary_handles_missing_file_as_uninitialized() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = workflow_test_root("current-state-summary-missing");
    let _ = fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));

    let summary = read_current_state_summary().unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(summary, "미초기화");
}

#[test]
fn tui_read_only_tail_accepts_legacy_prefix_before_chained_suffix() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = workflow_test_root("tui-read-tail-legacy-prefix");
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    fs::create_dir_all(paths::state_dir()).unwrap();
    let identity = ledger::fresh_identity();
    let path = paths::runtime_ledger_file();
    let legacy_prefix = (0..62)
        .map(|index| {
            format!(
                "{}\n",
                ledger::new_event_for(
                    &identity,
                    "legacy.event",
                    &format!("legacy {index}"),
                    "safe"
                )
                .to_json_line()
            )
        })
        .collect::<String>();
    fs::write(&path, &legacy_prefix).unwrap();
    let mut previous = format!(
        "legacy:{}",
        crate::runtime_core::workflow::storage_compat::ledger::sha256_bytes(
            legacy_prefix.as_bytes()
        )
    );
    for index in 0..61 {
        previous = crate::runtime_core::workflow::storage_compat::ledger::append_canonical_event(
            &path,
            &ledger::new_event_for(
                &identity,
                "chained.event",
                &format!("chained {index}"),
                "safe",
            ),
            &previous,
        )
        .unwrap();
    }
    fs::write(
        path.with_extension("jsonl.head"),
        format!(
            "{{\"schema_version\":1,\"event_count\":123,\"last_event_hash\":\"{previous}\"}}\n"
        ),
    )
    .unwrap();

    let tail = ledger::read_runtime_tail_read_only(80, 2 * 1024 * 1024).unwrap();
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

    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, original.replacen("legacy 0", "legacy x", 1)).unwrap();
    let error = ledger::read_runtime_tail_read_only(80, 2 * 1024 * 1024).unwrap_err();
    assert!(error.message.contains("adjacent hash chain 불일치"));

    fs::write(&path, &original).unwrap();
    let first_chained_offset = original.find("{\"schema_version\":2").unwrap();
    let budget = u64::try_from(original.len() - (first_chained_offset - 5)).unwrap();
    let error = ledger::read_runtime_tail_read_only(80, budget).unwrap_err();
    assert!(error
        .message
        .contains("legacy prefix가 read-only byte budget 안에 없습니다"));

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn current_state_is_isolated_per_project_under_shared_data_home() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = workflow_test_root("current-state-project-isolation");
    let data = root.join("data");
    let project_a = root.join("project-a");
    let project_b = root.join("project-b");
    fs::create_dir_all(&project_a).unwrap();
    fs::create_dir_all(&project_b).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", &data);

    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_a);
    let state_a = paths::current_state_file();
    let identity_a = initialize().unwrap().identity;

    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_b);
    let state_b = paths::current_state_file();
    let identity_b = initialize().unwrap().identity;

    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_a);
    let restored_a = initialize().unwrap().identity;
    let restored_lease = current_state_lease_view().unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_ne!(state_a, state_b);
    assert_eq!(identity_a, restored_a);
    assert_ne!(identity_a.project_id, identity_b.project_id);
    assert!(restored_lease.revision >= 2);
}

#[test]
fn unrelated_legacy_current_state_does_not_block_project_initialization() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = workflow_test_root("unrelated-legacy-current-state");
    let data = root.join("data");
    let old_project = root.join("old-project");
    let current_project = root.join("current-project");
    fs::create_dir_all(data.join("state")).unwrap();
    fs::create_dir_all(&old_project).unwrap();
    fs::create_dir_all(&current_project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    std::env::set_var("RPOTATO_PROJECT_ROOT", &old_project);
    let old_identity = ledger::fresh_identity();
    let mut legacy = CurrentStateSnapshot {
        schema_version: 2,
        revision: 1,
        previous_artifact_hash: "none".to_string(),
        project_id: old_identity.project_id,
        project_root: old_identity.project_root,
        session_id: old_identity.session_id,
        active_workflow: None,
        parent_session_id: None,
        branch_from_event_id: None,
        compaction_boundary: None,
        resume_source: None,
        ledger_binding: ledger::LedgerBinding {
            event_count: 0,
            event_id: None,
            event_hash: "root".to_string(),
        },
        artifact_hash: String::new(),
        legacy_canonical_hash: None,
    };
    legacy.artifact_hash = sha256_text(&render_current_state_v2_payload(&legacy));
    fs::write(
        paths::legacy_current_state_file(),
        render_current_state_v2(&legacy),
    )
    .unwrap();

    std::env::set_var("RPOTATO_PROJECT_ROOT", &current_project);
    let initialized = initialize().unwrap();
    let current_path = paths::current_state_file();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(
        initialized.identity.project_root,
        current_project.display().to_string()
    );
    assert!(current_path.starts_with(current_project.join(".rpotato/state")));
}

#[test]
fn divergent_project_current_state_is_not_silently_rebound() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = workflow_test_root("divergent-project-current-state");
    let data = root.join("data");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    initialize().unwrap();

    let current_path = paths::current_state_file();
    let mut snapshot = parse_current_state(
        &fs::read_to_string(&current_path).unwrap(),
        "divergent current-state fixture",
    )
    .unwrap();
    snapshot.ledger_binding.event_hash = "0".repeat(64);
    snapshot.artifact_hash = sha256_text(&render_current_state_v2_payload(&snapshot));
    fs::write(&current_path, render_current_state_v2(&snapshot)).unwrap();
    let current_before = fs::read(&current_path).unwrap();
    let ledger_before = fs::read(paths::runtime_ledger_file()).unwrap();

    let error = initialize().unwrap_err();

    assert!(error
        .message
        .contains("current-state ledger ancestor id/hash binding 불일치"));
    assert_eq!(fs::read(&current_path).unwrap(), current_before);
    assert_eq!(
        fs::read(paths::runtime_ledger_file()).unwrap(),
        ledger_before
    );

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn classifies_corrupt_current_state() {
    let identity = RuntimeIdentity {
        project_id: "project-a".to_string(),
        session_id: "session-a".to_string(),
        project_root: ".".to_string(),
    };

    assert_eq!(
        classify_current_state("not-json", &identity),
        CurrentStateStatus::Corrupt
    );
}

#[test]
fn classifies_stale_project_current_state() {
    let identity = RuntimeIdentity {
        project_id: "project-a".to_string(),
        session_id: "session-a".to_string(),
        project_root: ".".to_string(),
    };
    let contents = "{\n  \"schema_version\": 1,\n  \"project_id\": \"project-b\",\n  \"project_root\": \".\",\n  \"session_id\": \"session-a\",\n  \"active_workflow\": null,\n  \"parent_session_id\": null,\n  \"branch_from_event_id\": null,\n  \"compaction_boundary\": null,\n  \"resume_source\": null,\n  \"terminal_states\": [\"complete\", \"failed\", \"cancelled\"]\n}\n";

    assert_eq!(
        classify_current_state(contents, &identity),
        CurrentStateStatus::StaleProject
    );
}

#[test]
fn prepared_workflow_pair_and_single_current_image_are_deterministic() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("prepared-workflow-pair", |_| {
        let workflow = create_workflow("prepared workflow pair").unwrap();
        let guard = WorkflowCheckpointGuard::acquire(&workflow.workflow_id).unwrap();
        let current = guard.load_current().unwrap();
        let mut approved = current.clone();
        approved.phase = "approved".to_string();
        approved.approval_state = "approved".to_string();
        let r1 = guard.prepare_revision(&current, approved).unwrap();
        let mut pending = r1.record.clone();
        pending.phase = "pending-verification-approval".to_string();
        pending.approval_state = "applied".to_string();
        pending.verification_approval_state = "pending".to_string();
        let r2 = guard.prepare_revision(&r1.record, pending).unwrap();

        assert_eq!(r1.record.revision, current.revision + 1);
        assert_eq!(r2.record.revision, current.revision + 2);
        assert!(r1.pointer_bytes.ends_with("}\n"));
        assert!(r2
            .pointer_bytes
            .contains(&format!("\"committed_revision\": {}", r2.record.revision)));
        assert_ne!(r1.pointer_member_id, r2.pointer_member_id);
        assert_ne!(r1.snapshot_member_id, r2.snapshot_member_id);

        let before = ledger::validated_ledger_binding().unwrap();
        let final_binding = ledger::LedgerBinding {
            event_count: before.event_count + 10,
            event_id: Some("event-final-prepared".to_string()),
            event_hash: "f".repeat(64),
        };
        let current_image = prepare_current_image(&r2.record, &final_binding).unwrap();
        assert_eq!(
            current_image.revision,
            current_state_lease_view().unwrap().revision + 1
        );
        assert!(current_image.bytes.contains("\"schema_version\":2"));
        assert!(current_image
            .bytes
            .contains(&format!("\"revision\":{}", current_image.revision)));
        assert!(current_image.bytes.contains("event-final-prepared"));
    });
}

#[test]
fn prepared_current_image_rejects_same_revision_different_hash() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("prepared-current-cas", |_| {
        let path = paths::current_state_file();
        let body = fs::read_to_string(&path).unwrap();
        let before = parse_current_state(&body, "prepared current CAS before").unwrap();
        let mut forged = before.clone();
        forged.resume_source = Some("concurrent-valid-state".to_string());
        forged.artifact_hash = sha256_text(&render_current_state_v2_payload(&forged));
        let forged_body = render_current_state_v2(&forged);
        fs::write(&path, &forged_body).unwrap();
        let prepared = PreparedCurrentImage {
            path: path.clone(),
            stored_path: "state/current-state.json".to_string(),
            artifact_id: "current-image-future".to_string(),
            bytes: body,
            revision: before.revision + 1,
        };

        let error =
            install_current_image(&prepared, before.revision, &before.artifact_hash).unwrap_err();

        assert!(error.message.contains("exact CAS conflict"));
        assert_eq!(fs::read_to_string(path).unwrap(), forged_body);
    });
}

#[test]
fn current_state_v2_has_exact_order_hash_and_ledger_binding() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = workflow_test_root("current-state-v2");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);

    initialize().unwrap();
    let body = fs::read_to_string(paths::current_state_file()).unwrap();
    let snapshot = parse_current_state(&body, "current-state v2 fixture").unwrap();
    let lease = current_state_lease_view().unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(snapshot.schema_version, 2);
    assert_eq!(snapshot.revision, 1);
    assert_eq!(snapshot.previous_artifact_hash, "none");
    assert_eq!(snapshot.ledger_binding.event_count, 1);
    assert_eq!(lease.artifact_hash, snapshot.artifact_hash);
    assert_eq!(body, render_current_state_v2(&snapshot));
}

#[test]
fn exact_v1_is_promoted_once_before_lease() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = workflow_test_root("current-state-v1-promotion");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    ensure_layout().unwrap();
    let identity = ledger::fresh_identity();
    let legacy = format!(
            "{{\n  \"schema_version\": 1,\n  \"project_id\": \"{}\",\n  \"project_root\": \"{}\",\n  \"session_id\": \"{}\",\n  \"active_workflow\": null,\n  \"parent_session_id\": null,\n  \"branch_from_event_id\": null,\n  \"compaction_boundary\": null,\n  \"resume_source\": null,\n  \"terminal_states\": [\"complete\", \"failed\", \"cancelled\"]\n}}\n",
            identity.project_id, identity.project_root, identity.session_id
        );
    fs::write(paths::current_state_file(), &legacy).unwrap();
    let legacy_value = strict_json::parse_value(&legacy, "legacy").unwrap();
    let legacy_hash = sha256_text(&strict_json::render_compact(&legacy_value));

    let first = current_state_lease_view().unwrap();
    let first_body = fs::read_to_string(paths::current_state_file()).unwrap();
    let second = current_state_lease_view().unwrap();
    let second_body = fs::read_to_string(paths::current_state_file()).unwrap();
    let promoted = parse_current_state(&first_body, "promoted").unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(promoted.schema_version, 2);
    assert_eq!(promoted.revision, 1);
    assert_eq!(promoted.previous_artifact_hash, legacy_hash);
    assert_eq!(first, second);
    assert_eq!(first_body, second_body);
}

#[test]
fn current_state_v1_promotion_crash_matrix_is_idempotent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in ["after-temp-sync", "after-rename", "after-parent-sync"] {
        let root = workflow_test_root(&format!("current-state-v1-promotion-{point}"));
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        ensure_layout().unwrap();
        let identity = ledger::fresh_identity();
        let legacy = format!(
                "{{\"schema_version\":1,\"project_id\":\"{}\",\"project_root\":\"{}\",\"session_id\":\"{}\",\"active_workflow\":null,\"parent_session_id\":null,\"branch_from_event_id\":null,\"compaction_boundary\":null,\"resume_source\":null,\"terminal_states\":[\"complete\",\"failed\",\"cancelled\"]}}",
                identity.project_id, identity.project_root, identity.session_id
            );
        fs::write(paths::current_state_file(), &legacy).unwrap();
        std::env::set_var("RPOTATO_TEST_CURRENT_STATE_PROMOTION_FAULT", point);

        let error = current_state_lease_view().unwrap_err();
        assert!(error
            .message
            .contains("injected current-state promotion fault"));
        std::env::remove_var("RPOTATO_TEST_CURRENT_STATE_PROMOTION_FAULT");

        let first = current_state_lease_view().unwrap();
        let first_body = fs::read_to_string(paths::current_state_file()).unwrap();
        let second = current_state_lease_view().unwrap();
        let second_body = fs::read_to_string(paths::current_state_file()).unwrap();
        let promoted = parse_current_state_v2(&first_body, "promoted restart").unwrap();

        assert_eq!(promoted.revision, 1, "fault point {point}");
        assert_eq!(first, second, "fault point {point}");
        assert_eq!(first_body, second_body, "fault point {point}");
        assert!(!paths::current_state_v2_promotion_temp().exists());
        assert!(!paths::runtime_ledger_file().exists());

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn corrupt_current_state_blocks_canonical_mutation() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = workflow_test_root("corrupt-state-mutation");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    fs::create_dir_all(paths::current_state_dir()).unwrap();
    fs::write(paths::current_state_file(), b"not-json").unwrap();

    let event_error = record_event("test.mutation", "blocked", "safe").unwrap_err();
    let workflow_error = create_workflow("must not start").unwrap_err();
    let ledger_exists = paths::runtime_ledger_file().exists();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);
    assert_eq!(event_error.code, 3);
    assert_eq!(workflow_error.code, 3);
    assert!(!ledger_exists);
}

#[test]
fn sqlite_only_session_is_removed_and_cannot_resume() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("sqlite-session-authority", |_| {
        let identity = ledger::validated_current_identity().unwrap();
        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
        connection
                .execute(
                    "INSERT INTO sessions (session_id, project_id, project_root, started_at_ms) VALUES (?1, ?2, ?3, 1)",
                    rusqlite::params!["session-sqlite-only", identity.project_id, identity.project_root],
                )
                .unwrap();
        drop(connection);

        let sessions = observability::session_history(20).unwrap();
        assert!(sessions
            .iter()
            .all(|session| session.session_id != "session-sqlite-only"));
        let error = session_resume_report("session-sqlite-only").unwrap_err();
        assert_eq!(error.code, 3);
        assert!(error.message.contains("canonical runtime ledger"));

        let connection = rusqlite::Connection::open(paths::observability_db_file()).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE session_id = 'session-sqlite-only'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[test]
fn session_list_does_not_create_current_state_when_history_is_empty() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-session-list-empty-test-{}",
        std::process::id()
    ));
    let project_root = root.join("project");
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    let report = session_list_report().unwrap();
    let current_state_exists = paths::current_state_file().exists();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");

    assert!(report.contains("sessions: 없음"));
    assert!(!current_state_exists);
}

#[test]
fn session_resume_selects_existing_history_entry() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-session-resume-test-{}",
        std::process::id()
    ));
    let project_root = root.join("project");
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

    let new_report = session_new_report().unwrap();
    let session_id = new_report
        .lines()
        .find_map(|line| line.strip_prefix("- session id: "))
        .unwrap()
        .to_string();
    let list_report = session_list_report().unwrap();
    let resume_report = session_resume_report(&session_id).unwrap();
    let current_state = fs::read_to_string(paths::current_state_file()).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");

    assert!(list_report.contains(&session_id));
    assert!(resume_report.contains("session resume 결과"));
    assert!(current_state.contains(&format!("\"session_id\":\"{session_id}\"")));
    assert!(current_state.contains("\"resume_source\":\"session-history\""));
}

#[test]
fn tui_session_selection_revalidates_lease_under_lock_and_reuses_receipt() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("tui-session-selection-lease", |_| {
        let initial = ledger::validated_current_identity().unwrap();
        session_new_report().unwrap();
        let intent_id = "intent-session-select-exact-0001";
        let lease =
            crate::app::tui_adapter::canonical_selection_lease(&initial.session_id).unwrap();

        let first = session_resume_report_for_tui(&initial.session_id, intent_id, &lease)
            .unwrap()
            .unwrap();
        let after_first = fs::read_to_string(paths::current_state_file()).unwrap();
        let events_after_first = ledger::read_runtime_events().unwrap();
        let first_receipts = events_after_first
            .iter()
            .filter(|event| {
                event.event_type == "session.resume.selected"
                    && event.details.contains(&format!("intent_id={intent_id}"))
            })
            .count();

        let retry = session_resume_report_for_tui(&initial.session_id, intent_id, &lease)
            .unwrap()
            .unwrap();
        let after_retry = fs::read_to_string(paths::current_state_file()).unwrap();
        let retry_receipts = ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| {
                event.event_type == "session.resume.selected"
                    && event.details.contains(&format!("intent_id={intent_id}"))
            })
            .count();

        assert_eq!(first, retry);
        assert_eq!(after_first, after_retry);
        assert_eq!(first_receipts, 1);
        assert_eq!(retry_receipts, 1);

        let stale_lease =
            crate::app::tui_adapter::canonical_selection_lease(&initial.session_id).unwrap();
        record_event("test.selection.predecessor", "advance predecessor", "safe").unwrap();
        let before_stale_events = ledger::read_runtime_events().unwrap().len();
        assert!(session_resume_report_for_tui(
            &initial.session_id,
            "intent-session-select-stale-0002",
            &stale_lease,
        )
        .unwrap()
        .is_none());
        assert_eq!(
            ledger::read_runtime_events().unwrap().len(),
            before_stale_events
        );
    });
}
