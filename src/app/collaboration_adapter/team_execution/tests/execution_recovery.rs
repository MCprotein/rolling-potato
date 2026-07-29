#[test]
fn normal_pressure_executes_all_members_in_parallel_without_parent_merge() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_team();
    record_sample("normal");
    reset_runner_counters();

    let report = execute_with("team-execution", fake_preflight, fake_runner).unwrap();
    let team = team_state::load_state("team-execution").unwrap();
    let parent_after = state::load_workflow(&parent.workflow_id).unwrap();

    assert!(report.contains("status: workers-completed"));
    assert!(report.contains("execution mode: parallel"));
    assert!(report.contains("completed members: 2"));
    assert!(MAX_ACTIVE_RUNNERS.load(Ordering::SeqCst) >= 2);
    assert_eq!(team.stage, team_state::TeamStage::Execute);
    assert_eq!(parent_after.revision, parent.revision);
    assert!(parent_after.skill_evidence.is_empty());
}

#[test]
fn dispatch_retry_resumes_fully_admitted_workers_without_duplicate_admission() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_team();
    record_sample("normal");
    let admitted = admit_without_execute_transition();
    let original_ids = admitted
        .iter()
        .map(|member| member.subagent_id().to_string())
        .collect::<Vec<_>>();
    drop(admitted);

    let report = execute_with("team-execution", fake_preflight, fake_runner).unwrap();
    let records = subagent::records_for_parent(&parent.workflow_id).unwrap();
    let admitted_events = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == "team.worker.admitted")
        .count();

    assert!(report.contains("status: workers-completed"));
    assert_eq!(records.len(), 2);
    assert!(records
        .iter()
        .all(|record| original_ids.contains(&record.subagent_id)));
    assert!(records
        .iter()
        .all(|record| record.status == subagent::SubagentStatus::Completed));
    assert_eq!(admitted_events, 2);
}

#[test]
fn execute_retry_terminalizes_interrupted_running_workers_without_replay() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_team();
    record_sample("normal");
    let admitted = admit_without_execute_transition();
    team_state::advance_state("team-execution", team_state::TeamStage::Execute, None, None)
        .unwrap();
    let prepared = subagent::prepare_team_members(admitted).unwrap();
    drop(prepared);
    RECOVERY_RUNNERS.store(0, Ordering::SeqCst);

    let error = execute_with("team-execution", fake_preflight, counting_runner).unwrap_err();
    let team = team_state::load_state("team-execution").unwrap();
    let records = subagent::records_for_parent(&parent.workflow_id).unwrap();

    assert!(error.message.contains("cannot be replayed safely"));
    assert_eq!(team.stage, team_state::TeamStage::Failed);
    assert_eq!(RECOVERY_RUNNERS.load(Ordering::SeqCst), 0);
    assert!(records.iter().all(|record| {
        record.status == subagent::SubagentStatus::Failed
            && record.failure_code == "interrupted-no-replay"
    }));
}

#[test]
fn execute_retry_rebuilds_missing_completion_receipts_idempotently() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_team();
    record_sample("normal");
    let admitted = admit_without_execute_transition();
    team_state::advance_state("team-execution", team_state::TeamStage::Execute, None, None)
        .unwrap();
    let mut completed = admitted
        .into_iter()
        .map(|member| {
            subagent::execute_admitted_team_member_with(member, |prompt, max_tokens, timeout| {
                fake_runner(prompt, max_tokens, timeout, "team-execution")
            })
            .unwrap()
        })
        .collect::<Vec<_>>();
    completed.sort_by_key(|member| member.lane);
    let identity = ledger::validated_current_identity().unwrap();
    append_action_event(&identity, "team-execution", &completed[0], None).unwrap();

    let report = execute_with("team-execution", fake_preflight, counting_runner).unwrap();
    let reconciliation =
        super::super::team_reconciliation::reconcile_report("team-execution").unwrap();
    let events = ledger::read_runtime_events().unwrap();

    assert!(report.contains("completed members: 2"));
    assert!(reconciliation.contains("stop gate: passed"));
    assert_eq!(RECOVERY_RUNNERS.load(Ordering::SeqCst), 0);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "team.worker.action-owned")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "team.worker.completed")
            .count(),
        2
    );
    assert_eq!(
        state::load_workflow(&parent.workflow_id).unwrap().revision,
        parent.revision + 1
    );
}

#[test]
fn cancel_cannot_cross_the_admission_operation_barrier() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    initialize_team();
    ADMISSION_BARRIER_READY.store(false, Ordering::SeqCst);
    ADMISSION_BARRIER_RELEASE.store(false, Ordering::SeqCst);
    let execute = std::thread::spawn(|| {
        execute_with("team-execution", admission_barrier_preflight, fake_runner)
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !ADMISSION_BARRIER_READY.load(Ordering::SeqCst) {
        assert!(
            std::time::Instant::now() < deadline,
            "execute did not reach admission barrier"
        );
        std::thread::yield_now();
    }

    let cancel = team_state::cancel_report("team-execution").unwrap_err();
    assert!(cancel.message.contains("team operation lock 차단"));
    assert!(!team_state::cancellation_requested("team-execution").unwrap());
    ADMISSION_BARRIER_RELEASE.store(true, Ordering::SeqCst);
    let report = execute.join().unwrap().unwrap();
    let records = subagent::records_for_parent(
        &team_state::load_state("team-execution")
            .unwrap()
            .parent_workflow_id,
    )
    .unwrap();

    assert!(report.contains("status: workers-completed"));
    assert!(records
        .iter()
        .all(|record| record.status == subagent::SubagentStatus::Completed));
}
