use super::*;

use crate::adapters::filesystem::atomic_write::atomic_replace_bytes;

#[test]
fn atomic_replace_creates_parent_and_replaces_existing_bytes() {
    let root = workflow_test_root("atomic-replace");
    let target = root.join("nested/artifact.json");

    atomic_replace_bytes(&target, b"first").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"first");

    atomic_replace_bytes(&target, b"second").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"second");
    assert_eq!(
        fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1,
        "atomic replacement must not leave temporary files behind"
    );

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn atomic_replace_preserves_existing_permissions() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let root = workflow_test_root("atomic-permissions");
    fs::create_dir_all(&root).unwrap();
    let target = root.join("artifact.json");
    fs::write(&target, b"first").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();

    atomic_replace_bytes(&target, b"second").unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"second");
    assert_eq!(fs::metadata(&target).unwrap().mode() & 0o777, 0o640);

    let _ = fs::remove_dir_all(root);
}

#[cfg(windows)]
#[test]
fn atomic_replace_supports_long_new_and_existing_windows_targets() {
    let root = workflow_test_root("atomic-long-windows");
    let mut parent = root.clone();
    for index in 0..4 {
        parent.push(format!("segment-{index}-{}", "x".repeat(48)));
    }
    fs::create_dir_all(&parent).unwrap();
    let target = parent.join(format!("artifact-{}.json", "y".repeat(48)));
    assert!(target.as_os_str().len() > 260);

    atomic_replace_bytes(&target, b"first").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"first");
    atomic_replace_bytes(&target, b"second").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"second");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn legacy_v2_chain_is_preserved_and_next_checkpoint_appends_v4() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("workflow-v2-upgrade", |_| {
        let mut legacy = WorkflowRecord::new(&ledger::fresh_identity(), "legacy pending workflow");
        legacy.revision = 1;
        legacy.previous_hash = "none".to_string();
        legacy.phase = "pending-approval".to_string();
        legacy.approval_state = "pending".to_string();
        legacy.artifact_hash = sha256_text(&workflow_payload_v2(&legacy));
        let snapshot = paths::project_workflow_snapshot_file(&legacy.workflow_id, 1);
        atomic_replace_bytes(&snapshot, render_workflow_v2(&legacy).as_bytes()).unwrap();
        append_workflow_checkpoint_event(&legacy).unwrap();
        write_workflow_pointer_for_schema(&legacy, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();
        let legacy_bytes = fs::read(&snapshot).unwrap();

        let mut loaded = load_workflow(&legacy.workflow_id).unwrap();
        assert_eq!(loaded.revision, 1);
        assert_eq!(loaded.verification_approval_state, "not-issued");
        loaded.result_summary = "v2 workflow upgraded".to_string();
        let upgraded = checkpoint_workflow(loaded.clone(), loaded.revision).unwrap();

        assert_eq!(upgraded.revision, 2);
        assert_eq!(upgraded.previous_hash, legacy.artifact_hash);
        assert_eq!(fs::read(&snapshot).unwrap(), legacy_bytes);
        let pointer =
            fs::read_to_string(paths::project_workflow_file(&legacy.workflow_id)).unwrap();
        assert!(pointer.contains("\"schema_version\": 4"));
        assert!(pointer.contains("workflow-commit-v4"));
        let v4 = fs::read_to_string(paths::project_workflow_snapshot_file(
            &legacy.workflow_id,
            2,
        ))
        .unwrap();
        assert!(v4.contains("\"artifact_version\": \"workflow-v4\""));
        assert_eq!(load_workflow(&legacy.workflow_id).unwrap(), upgraded);
    });
}

#[test]
fn v3_loads_without_rewrite_and_next_checkpoint_persists_skill_state_as_v4() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("workflow-v3-upgrade", |_| {
        let mut v3 = WorkflowRecord::new(&ledger::fresh_identity(), "v3 workflow");
        v3.revision = 1;
        v3.previous_hash = "none".to_string();
        v3.phase = "model-pending".to_string();
        v3.artifact_hash = sha256_text(&workflow_payload_v3(&v3));
        let snapshot = paths::project_workflow_snapshot_file(&v3.workflow_id, 1);
        let v3_bytes = render_workflow_v3(&v3);
        atomic_replace_bytes(&snapshot, v3_bytes.as_bytes()).unwrap();
        append_workflow_checkpoint_event(&v3).unwrap();
        write_workflow_pointer_for_schema(&v3, PREVIOUS_WORKFLOW_SCHEMA_VERSION).unwrap();

        let mut loaded = load_workflow(&v3.workflow_id).unwrap();
        assert_eq!(fs::read_to_string(&snapshot).unwrap(), v3_bytes);
        assert!(loaded.active_skill_id.is_empty());
        assert!(loaded.skill_state.is_empty());

        loaded.active_skill_id = "built-in-plan".to_string();
        loaded.skill_invocation = "$plan --consensus".to_string();
        loaded.skill_state = "running".to_string();
        loaded.skill_completed_hooks = "session-start,preflight".to_string();
        loaded.skill_evidence = "artifact:plan-v1".to_string();
        loaded.skill_stop_criteria = "verified".to_string();
        let checkpointed = checkpoint_workflow(loaded.clone(), loaded.revision).unwrap();
        let restarted = load_workflow(&v3.workflow_id).unwrap();

        assert_eq!(restarted, checkpointed);
        assert_eq!(restarted.active_skill_id, "built-in-plan");
        assert_eq!(restarted.skill_invocation, "$plan --consensus");
        assert_eq!(restarted.skill_state, "running");
        assert_eq!(restarted.skill_completed_hooks, "session-start,preflight");
        assert_eq!(restarted.skill_evidence, "artifact:plan-v1");
        assert_eq!(restarted.skill_stop_criteria, "verified");
        assert_eq!(fs::read_to_string(&snapshot).unwrap(), v3_bytes);
        let pointer = fs::read_to_string(paths::project_workflow_file(&v3.workflow_id)).unwrap();
        assert!(pointer.contains("workflow-commit-v4"));
    });
}

#[test]
fn legacy_v2_complete_maps_split_approval_evidence_without_rewriting() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("workflow-v2-complete-map", |root| {
        let mut non_mutating = WorkflowRecord::new(
            &ledger::fresh_identity(),
            "legacy read-only complete workflow",
        );
        non_mutating.revision = 1;
        non_mutating.previous_hash = "none".to_string();
        non_mutating.phase = "complete".to_string();
        non_mutating.action_kind = "inspect-sources".to_string();
        non_mutating.approval_state = "not-required".to_string();
        non_mutating.artifact_hash = sha256_text(&workflow_payload_v2(&non_mutating));
        let parsed_non_mutating = parse_workflow_snapshot(
            &root.join("non-mutating-v2.json"),
            &render_workflow_v2(&non_mutating),
        )
        .unwrap();
        assert_eq!(parsed_non_mutating.approval_state, "not-required");
        assert_eq!(
            parsed_non_mutating.verification_approval_state,
            "not-issued"
        );

        let mut legacy = WorkflowRecord::new(&ledger::fresh_identity(), "legacy complete workflow");
        legacy.revision = 1;
        legacy.previous_hash = "none".to_string();
        legacy.phase = "complete".to_string();
        legacy.action_kind = "patch-proposal".to_string();
        legacy.approval_state = "approved".to_string();
        legacy.proposal_id = "patch-proposal-legacy".to_string();
        legacy.source_path = "src/lib.rs".to_string();
        legacy.after_hash = "a".repeat(64);
        legacy.evidence_id = "evidence-legacy".to_string();
        legacy.artifact_hash = sha256_text(&workflow_payload_v2(&legacy));
        let snapshot = paths::project_workflow_snapshot_file(&legacy.workflow_id, 1);
        let bytes = render_workflow_v2(&legacy);
        atomic_replace_bytes(&snapshot, bytes.as_bytes()).unwrap();
        append_workflow_checkpoint_event(&legacy).unwrap();
        write_workflow_pointer_for_schema(&legacy, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();

        let loaded = load_workflow(&legacy.workflow_id).unwrap();

        assert_eq!(loaded.phase, "complete");
        assert_eq!(loaded.approval_state, "applied");
        assert_eq!(loaded.verification_approval_state, "approved");
        assert_eq!(fs::read_to_string(snapshot).unwrap(), bytes);
    });
}

#[test]
fn interrupted_legacy_v2_transaction_without_prepared_event_fails_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("workflow-v2-transaction", |_| {
        let mut first =
            WorkflowRecord::new(&ledger::fresh_identity(), "legacy transaction workflow");
        first.revision = 1;
        first.previous_hash = "none".to_string();
        first.phase = "pending-approval".to_string();
        first.approval_state = "pending".to_string();
        first.artifact_hash = sha256_text(&workflow_payload_v2(&first));
        atomic_replace_bytes(
            &paths::project_workflow_snapshot_file(&first.workflow_id, 1),
            render_workflow_v2(&first).as_bytes(),
        )
        .unwrap();
        append_workflow_checkpoint_event(&first).unwrap();
        write_workflow_pointer_for_schema(&first, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();

        let mut second = first.clone();
        second.revision = 2;
        second.previous_hash = first.artifact_hash.clone();
        second.phase = "verification-started".to_string();
        second.approval_state = "approved".to_string();
        second.proposal_id = "patch-proposal-legacy-transaction".to_string();
        second.artifact_hash = sha256_text(&workflow_payload_v2(&second));
        let transaction = render_workflow_v2(&second);
        atomic_replace_bytes(
            &paths::project_workflow_transaction_file(&second.workflow_id),
            transaction.as_bytes(),
        )
        .unwrap();

        let error = load_workflow(&second.workflow_id).unwrap_err();

        assert!(error.message.contains("exact prepared semantic event"));
        assert!(!paths::project_workflow_snapshot_file(&second.workflow_id, 2).exists());
        let pointer =
            fs::read_to_string(paths::project_workflow_file(&second.workflow_id)).unwrap();
        assert!(pointer.contains("workflow-commit-v2"));
        assert_eq!(
            fs::read_to_string(paths::project_workflow_transaction_file(
                &second.workflow_id
            ))
            .unwrap(),
            transaction
        );
        assert_eq!(
            ledger::workflow_checkpoints(&second.workflow_id)
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn workflow_recovery_rejects_unbound_previous_hash_before_append() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("workflow-recovery-binding", |_| {
        let mut first = WorkflowRecord::new(&ledger::fresh_identity(), "recovery binding workflow");
        first.revision = 1;
        first.previous_hash = "none".to_string();
        first.artifact_hash = sha256_text(&workflow_payload_v2(&first));
        atomic_replace_bytes(
            &paths::project_workflow_snapshot_file(&first.workflow_id, 1),
            render_workflow_v2(&first).as_bytes(),
        )
        .unwrap();
        append_workflow_checkpoint_event(&first).unwrap();
        write_workflow_pointer_for_schema(&first, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();

        let mut forged = first.clone();
        forged.revision = 2;
        forged.previous_hash = "f".repeat(64);
        forged.artifact_hash = sha256_text(&workflow_payload(&forged));
        atomic_replace_bytes(
            &paths::project_workflow_transaction_file(&forged.workflow_id),
            render_workflow(&forged).as_bytes(),
        )
        .unwrap();

        let error = load_workflow(&forged.workflow_id).unwrap_err();

        assert_eq!(error.code, 3);
        assert!(!paths::project_workflow_snapshot_file(&forged.workflow_id, 2).exists());
        assert_eq!(
            ledger::workflow_checkpoints(&forged.workflow_id)
                .unwrap()
                .len(),
            1
        );
    });
}

#[test]
fn workflow_chain_rejects_v3_to_v2_schema_downgrade() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("workflow-schema-downgrade", |_| {
        let mut first = WorkflowRecord::new(&ledger::fresh_identity(), "schema downgrade workflow");
        first.revision = 1;
        first.previous_hash = "none".to_string();
        first.artifact_hash = sha256_text(&workflow_payload_v3(&first));
        atomic_replace_bytes(
            &paths::project_workflow_snapshot_file(&first.workflow_id, 1),
            render_workflow_v3(&first).as_bytes(),
        )
        .unwrap();
        append_workflow_checkpoint_event(&first).unwrap();
        write_workflow_pointer_for_schema(&first, PREVIOUS_WORKFLOW_SCHEMA_VERSION).unwrap();
        let mut downgraded = first.clone();
        downgraded.revision = 2;
        downgraded.previous_hash = first.artifact_hash.clone();
        downgraded.artifact_hash = sha256_text(&workflow_payload_v2(&downgraded));
        atomic_replace_bytes(
            &paths::project_workflow_snapshot_file(&downgraded.workflow_id, 2),
            render_workflow_v2(&downgraded).as_bytes(),
        )
        .unwrap();
        append_workflow_checkpoint_event(&downgraded).unwrap();
        write_workflow_pointer_for_schema(&downgraded, LEGACY_WORKFLOW_SCHEMA_VERSION).unwrap();

        let error = load_workflow(&downgraded.workflow_id).unwrap_err();

        assert_eq!(error.code, 3);
        assert!(error.message.contains("fail-closed"));
    });
}

#[test]
fn terminal_pointer_cleanup_revalidates_stop_gate_before_clear() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("terminal-pointer-cleanup", |_| {
        let mut workflow = create_workflow("finish me").unwrap();
        workflow.phase = "complete".to_string();
        std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", "after-pointer");
        checkpoint_workflow(workflow.clone(), workflow.revision).unwrap_err();
        std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");

        assert_eq!(
            active_workflow_id().unwrap(),
            Some(workflow.workflow_id.clone())
        );
        let error = resume_report().unwrap_err();
        assert!(error.message.contains("proposal"));
        let current = fs::read_to_string(paths::current_state_file()).unwrap();
        assert!(current.contains(&workflow.workflow_id));
        assert!(load_workflow(&workflow.workflow_id).unwrap().is_terminal());
    });
}

#[test]
fn all_artifacts_are_scanned_and_multiple_active_workflows_fail_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("multi-active", |_| {
        let first = create_workflow("first").unwrap();
        let second = create_workflow("second").unwrap();
        assert_ne!(first.workflow_id, second.workflow_id);

        let error = active_workflow_id().unwrap_err();
        assert_eq!(error.code, 3);
        assert!(error.message.contains("여러 non-terminal"));
    });
}

#[test]
fn state_status_reports_the_discovered_active_workflow() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("status-active", |_| {
        let workflow = create_workflow("status truth").unwrap();
        let report = status_report().unwrap();
        assert!(report.contains(&format!("active workflow: {}", workflow.workflow_id)));
    });
}

#[test]
fn snapshot_tamper_fails_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("snapshot-tamper", |_| {
        let workflow = create_workflow("tamper me").unwrap();
        let snapshot = paths::project_workflow_snapshot_file(&workflow.workflow_id, 1);
        let mut body = fs::read_to_string(&snapshot).unwrap();
        body = body.replace("model-pending", "approved");
        fs::write(&snapshot, body).unwrap();

        let error = load_workflow(&workflow.workflow_id).unwrap_err();
        assert_eq!(error.code, 3);
        assert!(error.message.contains("fail-closed"));
    });
}

#[test]
fn workflow_recovery_bounds_transaction_pointer_and_revision_snapshot_reads() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("workflow-recovery-read-bounds", |_| {
        let workflow = create_workflow("bounded workflow recovery").unwrap();
        let transaction = paths::project_workflow_transaction_file(&workflow.workflow_id);
        fs::write(
            &transaction,
            vec![b'x'; usize::try_from(MAX_WORKFLOW_SNAPSHOT_BYTES).unwrap() + 1],
        )
        .unwrap();
        let transaction_error = recover_workflow_transaction(&workflow.workflow_id).unwrap_err();
        assert!(transaction_error
            .message
            .contains("regular-file/byte budget"));

        let pointer = paths::project_workflow_file(&workflow.workflow_id);
        let pointer_body = fs::read(&pointer).unwrap();
        let snapshot =
            paths::project_workflow_snapshot_file(&workflow.workflow_id, workflow.revision);
        fs::write(&transaction, fs::read(&snapshot).unwrap()).unwrap();
        fs::write(
            &pointer,
            vec![b'x'; usize::try_from(MAX_WORKFLOW_POINTER_BYTES).unwrap() + 1],
        )
        .unwrap();
        let pointer_error = recover_workflow_transaction(&workflow.workflow_id).unwrap_err();
        assert!(pointer_error.message.contains("regular-file/byte budget"));

        fs::remove_file(&transaction).unwrap();
        fs::write(&pointer, pointer_body).unwrap();
        fs::write(
            &snapshot,
            vec![b'x'; usize::try_from(MAX_WORKFLOW_SNAPSHOT_BYTES).unwrap() + 1],
        )
        .unwrap();
        let snapshot_error = validate_workflow_chain(
            &workflow.workflow_id,
            workflow.revision,
            WORKFLOW_SCHEMA_VERSION,
        )
        .unwrap_err();
        assert!(snapshot_error.message.contains("regular-file/byte budget"));
    });
}

#[test]
fn ledger_ahead_of_committed_pointer_fails_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    with_workflow_env("ledger-ahead", |_| {
        let workflow = create_workflow("stale latest checkpoint").unwrap();
        let identity = workflow_identity(&workflow);
        let forged_hash = "d".repeat(64);
        let event = ledger::new_event_for(
                &identity,
                "workflow.checkpoint",
                "forged uncommitted checkpoint",
                &format!(
                    "workflow_id={} revision=2 artifact_hash={forged_hash} previous_hash={} phase=approved action_id={} proposal_id=none evidence_id=none",
                    workflow.workflow_id, workflow.artifact_hash, workflow.action_id
                ),
            );
        ledger::append_event(&event).unwrap();

        let error = load_workflow(&workflow.workflow_id).unwrap_err();
        assert_eq!(error.code, 3);
        assert!(error.message.contains("ledger checkpoints: 2"));
    });
}
