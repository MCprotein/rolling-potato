#[test]
fn unknown_pressure_runs_every_member_sequentially_without_dropping_work() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_team();
    reset_runner_counters();

    let report = execute_with("team-execution", fake_preflight, fake_runner).unwrap();
    let team = team_state::load_state("team-execution").unwrap();

    assert!(report.contains("execution mode: sequential"));
    assert!(report.contains("requested lanes: 2"));
    assert!(report.contains("admitted lanes: 1"));
    assert!(report.contains("completed members: 2"));
    assert_eq!(MAX_ACTIVE_RUNNERS.load(Ordering::SeqCst), 1);
    assert_eq!(team.admitted_lanes, 1);
}

#[test]
fn critical_pressure_blocks_before_worker_admission_or_stage_change() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_team();
    record_sample("critical");

    let error = execute_with("team-execution", fake_preflight, fake_runner).unwrap_err();
    let team = team_state::load_state("team-execution").unwrap();
    let worker_events = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type.starts_with("team.worker."))
        .count();

    assert!(error.message.contains("resource admission 차단"));
    assert_eq!(team.stage, team_state::TeamStage::Plan);
    assert_eq!(worker_events, 0);
}

#[test]
fn executor_patch_is_rechecked_against_action_time_lane_ownership() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_executor_team();
    record_sample("normal");

    let report = execute_with("team-action", fake_preflight, patch_runner).unwrap();
    let action_event = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .find(|event| event.event_type == "team.worker.action-owned")
        .unwrap();

    assert!(report.contains("completed members: 1"));
    assert!(action_event.details.contains("lane=1"));
    assert!(action_event.details.contains("action=patch"));
    assert!(action_event.details.contains("target_path=src/main.rs"));
}

#[test]
fn worker_failure_collects_remaining_results_and_terminalizes_team() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_team();
    record_sample("normal");

    let error = execute_with("team-execution", fake_preflight, one_worker_fails).unwrap_err();
    let team = team_state::load_state("team-execution").unwrap();
    let completed_workers = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == "team.worker.completed")
        .count();

    assert!(error.message.contains("stage: failed"));
    assert!(error.message.contains("injected worker failure"));
    assert_eq!(team.stage, team_state::TeamStage::Failed);
    assert_eq!(completed_workers, 1);
}

#[test]
fn durable_cancellation_marker_reaches_every_sequential_worker() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_team();
    CANCEL_STARTED.store(false, Ordering::SeqCst);
    CANCEL_OBSERVERS.store(0, Ordering::SeqCst);

    let error = execute_with("team-execution", fake_preflight, cancelling_runner).unwrap_err();
    let team = team_state::load_state("team-execution").unwrap();
    let cancelled_workers = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == "team.subagent.cancelled")
        .count();

    assert!(error.message.contains("team execute cancelled"));
    assert_eq!(team.stage, team_state::TeamStage::Cancelled);
    assert_eq!(CANCEL_OBSERVERS.load(Ordering::SeqCst), 2);
    assert_eq!(cancelled_workers, 2);
}
