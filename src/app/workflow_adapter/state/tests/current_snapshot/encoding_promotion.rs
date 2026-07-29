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
