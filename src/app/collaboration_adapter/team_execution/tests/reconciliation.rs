#[test]
fn completed_team_reconciles_all_evidence_once_and_retries_idempotently() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_team();
    record_sample("normal");
    execute_with("team-execution", fake_preflight, fake_runner).unwrap();

    let report = super::super::team_reconciliation::reconcile_report("team-execution").unwrap();
    let completed = team_state::load_state("team-execution").unwrap();
    let merged_parent = state::load_workflow(&parent.workflow_id).unwrap();
    let first_hash = merged_parent.artifact_hash.clone();
    let retry = super::super::team_reconciliation::reconcile_report("team-execution").unwrap();
    let retried_parent = state::load_workflow(&parent.workflow_id).unwrap();
    let events = ledger::read_runtime_events().unwrap();

    assert!(report.contains("stop gate: passed"));
    assert!(retry.contains("status: completed"));
    assert_eq!(completed.stage, team_state::TeamStage::Complete);
    assert_eq!(merged_parent.revision, parent.revision + 1);
    assert_eq!(
        merged_parent
            .skill_evidence
            .split(',')
            .filter(|value| !value.is_empty())
            .count(),
        2
    );
    assert_eq!(retried_parent.artifact_hash, first_hash);
    assert!(paths::project_team_reconciliation_file("team-execution").is_file());
    for event_type in [
        "team.result-set.reconciled",
        "team.evidence.merged",
        "team.stop-gate.passed",
        "team.report.completed",
    ] {
        assert_eq!(
            events
                .iter()
                .filter(|event| event.event_type == event_type)
                .count(),
            1,
            "{event_type} must be idempotent"
        );
    }
}

#[test]
fn unresolved_validation_gap_blocks_before_parent_evidence_merge() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_team();
    record_sample("normal");
    execute_with("team-execution", fake_preflight, validation_gap_runner).unwrap();

    let error = super::super::team_reconciliation::reconcile_report("team-execution").unwrap_err();
    let blocked = team_state::load_state("team-execution").unwrap();
    let unchanged_parent = state::load_workflow(&parent.workflow_id).unwrap();

    assert!(error.message.contains("unresolved worker validation gaps"));
    assert_eq!(blocked.stage, team_state::TeamStage::Review);
    assert_eq!(unchanged_parent.revision, parent.revision);
    assert!(unchanged_parent.skill_evidence.is_empty());
    assert!(ledger::read_runtime_events()
        .unwrap()
        .iter()
        .any(|event| event.event_type == "team.stop-gate.failed"));
}

#[test]
fn source_change_after_worker_completion_blocks_before_parent_evidence_merge() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_team();
    record_sample("normal");
    execute_with("team-execution", fake_preflight, fake_runner).unwrap();
    fs::write(
        paths::project_root().join("src/main.rs"),
        "fn main() { println!(\"changed\"); }\n",
    )
    .unwrap();

    let error = super::super::team_reconciliation::reconcile_report("team-execution").unwrap_err();
    let blocked = team_state::load_state("team-execution").unwrap();
    let unchanged_parent = state::load_workflow(&parent.workflow_id).unwrap();

    assert!(error.message.contains("missing or stale worker evidence"));
    assert_eq!(blocked.stage, team_state::TeamStage::Review);
    assert_eq!(unchanged_parent.revision, parent.revision);
    assert!(unchanged_parent.skill_evidence.is_empty());
    assert!(ledger::read_runtime_events()
        .unwrap()
        .iter()
        .any(|event| event.event_type == "team.stop-gate.failed"));
}
