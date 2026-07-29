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
        let (line, event_hash) =
            crate::runtime_core::workflow::storage_compat::ledger::canonical_event_line(
                &ledger::new_event_for(
                    &identity,
                    "chained.event",
                    &format!("chained {index}"),
                    "safe",
                ),
                &previous,
            );
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(format!("{line}\n").as_bytes())
            .unwrap();
        previous = event_hash;
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
fn current_state_lease_releases_ledger_guard_before_loading_active_workflow() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("current-state-ledger-guard-scope", |_| {
        let workflow = create_workflow("ledger guard scope").unwrap();

        let lease = current_state_lease_view().unwrap();
        let binding = ledger::validated_ledger_binding().unwrap();

        assert_eq!(
            workflow.session_id,
            ledger::validated_current_identity().unwrap().session_id
        );
        assert!(lease.revision > 0);
        assert!(binding.event_count > 0);
    });
}
