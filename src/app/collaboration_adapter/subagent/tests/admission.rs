use super::*;

#[test]
fn admission_binds_active_parent_and_records_ordered_events() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap();
    let admitted = admitted.record;
    assert_eq!(admitted.status, SubagentStatus::Admitted);
    assert_eq!(admitted.revision, 2);
    assert_eq!(admitted.project_id, parent.project_id);
    assert_eq!(admitted.session_id, parent.session_id);
    assert_eq!(admitted.parent_workflow_id, parent.workflow_id);
    assert_eq!(admitted.parent_revision, parent.revision);
    assert_eq!(admitted.parent_artifact_hash, parent.artifact_hash);

    let lifecycle = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type.starts_with("team.subagent."))
        .map(|event| event.event_type)
        .collect::<Vec<_>>();
    assert_eq!(
        lifecycle,
        vec![
            "team.subagent.requested".to_string(),
            "team.subagent.admitted".to_string(),
        ]
    );
}

#[test]
fn admission_requires_parent_and_blocks_second_non_terminal_child() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    state::initialize().unwrap();
    assert!(admit_launch(launch("explore"))
        .unwrap_err()
        .message
        .contains("active non-terminal parent"));

    fs::create_dir_all(paths::project_root().join("src")).unwrap();
    fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
    state::create_workflow("subagent parent fixture").unwrap();
    let first = admit_launch(launch("explore")).unwrap().record;
    let error = admit_launch(launch("planner")).unwrap_err();
    assert!(error.message.contains("non-terminal child"));
    assert_eq!(
        records_for_parent(&first.parent_workflow_id).unwrap().len(),
        1
    );
}

#[test]
fn admission_rejects_terminal_parent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let mut terminal = parent.clone();
    terminal.phase = "complete".to_string();
    let terminal = state::checkpoint_workflow(terminal, parent.revision).unwrap();
    assert!(terminal.is_terminal());
    let error = admit_launch(launch("explore")).unwrap_err();
    assert!(error.message.contains("active non-terminal 상태"));
    assert!(records_for_parent(&parent.workflow_id).unwrap().is_empty());
}

#[test]
fn status_defaults_to_active_parent_and_cancel_is_idempotent() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap().record;
    let status = status_report(None).unwrap();
    assert!(status.contains(&admitted.subagent_id));
    assert!(status.contains("status: admitted"));

    let cancelled_report = cancel_report(&admitted.subagent_id).unwrap();
    assert!(cancelled_report.contains("action: cancelled"));
    let cancelled = load_record(&admitted.subagent_id).unwrap();
    assert_eq!(cancelled.status, SubagentStatus::Cancelled);
    assert_eq!(cancelled.revision, 3);

    let retry = cancel_report(&admitted.subagent_id).unwrap();
    assert!(retry.contains("already-cancelled-no-op"));
    assert_eq!(load_record(&admitted.subagent_id).unwrap().revision, 3);
}

#[test]
fn stale_running_child_recovers_as_failed_without_backend_replay() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_parent();
    let admitted = admit_launch(launch("explore")).unwrap().record;
    let mut running = admitted.clone();
    running
        .transition_to(SubagentStatus::Running, None)
        .unwrap();
    let running = checkpoint_record(running, admitted.revision).unwrap();

    let replacement = admit_launch(launch("planner")).unwrap().record;
    let recovered = load_record(&running.subagent_id).unwrap();
    assert_eq!(recovered.status, SubagentStatus::Failed);
    assert_eq!(recovered.failure_code, "interrupted-no-replay");
    assert_eq!(replacement.status, SubagentStatus::Admitted);
    assert_ne!(replacement.subagent_id, recovered.subagent_id);
}
