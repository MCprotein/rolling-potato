use super::*;

#[test]
fn canonical_state_round_trips_and_preserves_hash_chain() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let requested = create_record(record("explore")).unwrap();
    let mut admitted = requested.clone();
    admitted
        .transition_to(SubagentStatus::Admitted, None)
        .unwrap();
    let admitted = checkpoint_record(admitted, requested.revision).unwrap();
    let mut running = admitted.clone();
    running
        .transition_to(SubagentStatus::Running, None)
        .unwrap();
    let running = checkpoint_record(running, admitted.revision).unwrap();
    let mut completed = running.clone();
    completed.backend_event_id = "backend-event-test".to_string();
    completed.result_artifact_id = "result-test".to_string();
    completed.result_artifact_hash = "b".repeat(64);
    completed.evidence_id = "evidence-test".to_string();
    completed.evidence_hash = "c".repeat(64);
    completed
        .transition_to(SubagentStatus::Completed, None)
        .unwrap();
    let completed = checkpoint_record(completed, running.revision).unwrap();
    assert_eq!(completed.revision, 4);
    assert_eq!(load_record(&completed.subagent_id).unwrap(), completed);
    for revision in 1..=4 {
        assert!(paths::project_subagent_snapshot_file(&completed.subagent_id, revision).is_file());
    }
}

#[test]
fn stale_revision_and_immutable_binding_changes_fail_closed() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let requested = create_record(record("explore")).unwrap();
    let mut admitted = requested.clone();
    admitted
        .transition_to(SubagentStatus::Admitted, None)
        .unwrap();
    let admitted = checkpoint_record(admitted, requested.revision).unwrap();

    let mut stale = requested.clone();
    stale
        .transition_to(SubagentStatus::Cancelled, Some("user-cancelled"))
        .unwrap();
    assert!(checkpoint_record(stale, requested.revision)
        .unwrap_err()
        .message
        .contains("stale revision"));

    let mut forged = admitted.clone();
    forged.parent_workflow_id = "workflow-other".to_string();
    forged.transition_to(SubagentStatus::Running, None).unwrap();
    assert!(checkpoint_record(forged, admitted.revision)
        .unwrap_err()
        .message
        .contains("immutable"));
}

#[test]
fn tampered_current_or_snapshot_state_is_rejected() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let requested = create_record(record("explore")).unwrap();
    let current = paths::project_subagent_file(&requested.subagent_id);
    let original = fs::read_to_string(&current).unwrap();
    fs::write(&current, original.replace("requested", "admitted")).unwrap();
    assert!(load_record(&requested.subagent_id).is_err());

    fs::write(&current, &original).unwrap();
    let snapshot = paths::project_subagent_snapshot_file(&requested.subagent_id, 1);
    fs::write(&snapshot, original.replace("project-test", "project-evil")).unwrap();
    assert!(load_record(&requested.subagent_id).is_err());
}

#[test]
fn conflicting_preinstalled_snapshot_blocks_checkpoint() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let pending = record("explore");
    let path = paths::project_subagent_snapshot_file(&pending.subagent_id, 1);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "forged").unwrap();
    assert!(create_record(pending)
        .unwrap_err()
        .message
        .contains("snapshot 충돌"));
}
