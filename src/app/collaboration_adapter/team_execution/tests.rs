use super::*;
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::state;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static ACTIVE_RUNNERS: AtomicUsize = AtomicUsize::new(0);
static MAX_ACTIVE_RUNNERS: AtomicUsize = AtomicUsize::new(0);
static CANCEL_STARTED: AtomicBool = AtomicBool::new(false);
static CANCEL_OBSERVERS: AtomicUsize = AtomicUsize::new(0);
static ADMISSION_BARRIER_READY: AtomicBool = AtomicBool::new(false);
static ADMISSION_BARRIER_RELEASE: AtomicBool = AtomicBool::new(false);
static RECOVERY_RUNNERS: AtomicUsize = AtomicUsize::new(0);

fn initialize_team() -> state::WorkflowRecord {
    fs::create_dir_all(paths::project_root().join("src")).unwrap();
    fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
    state::initialize().unwrap();
    let parent = state::create_workflow("team execution parent").unwrap();
    let manifest = format!(
        "{{\"schema_version\":1,\"team_id\":\"team-execution\",\"parent_workflow_id\":\"{}\",\"members\":[{{\"lane\":1,\"id\":\"explore-1\",\"role\":\"explore\",\"task\":\"inspect the source\",\"tools\":[\"read_file\"],\"read_paths\":[\"src/main.rs\"],\"write_paths\":[],\"timeout_ms\":30000,\"max_tokens\":256}},{{\"lane\":2,\"id\":\"verifier-1\",\"role\":\"verifier\",\"task\":\"verify the source\",\"tools\":[\"read_file\"],\"read_paths\":[\"src/main.rs\"],\"write_paths\":[],\"timeout_ms\":30000,\"max_tokens\":256}}],\"write_policy\":\"single_writer\",\"merge_policy\":\"runtime_owned\",\"stop_gate\":\"evidence_required\"}}",
        parent.workflow_id,
    );
    fs::write(paths::project_root().join("team.json"), manifest).unwrap();
    team_state::plan_report("team.json").unwrap();
    parent
}

fn initialize_executor_team() {
    fs::create_dir_all(paths::project_root().join("src")).unwrap();
    fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
    state::initialize().unwrap();
    let parent = state::create_workflow("team executor parent").unwrap();
    let manifest = format!(
        "{{\"schema_version\":1,\"team_id\":\"team-action\",\"parent_workflow_id\":\"{}\",\"members\":[{{\"lane\":1,\"id\":\"executor-1\",\"role\":\"executor\",\"task\":\"prepare the bounded patch\",\"tools\":[\"read_file\",\"render_diff\"],\"read_paths\":[\"src/main.rs\"],\"write_paths\":[\"src/main.rs\"],\"timeout_ms\":30000,\"max_tokens\":256}}],\"write_policy\":\"single_writer\",\"merge_policy\":\"runtime_owned\",\"stop_gate\":\"evidence_required\"}}",
        parent.workflow_id,
    );
    fs::write(paths::project_root().join("team-action.json"), manifest).unwrap();
    team_state::plan_report("team-action.json").unwrap();
}

fn fake_preflight() -> Result<(), AppError> {
    Ok(())
}

fn admission_barrier_preflight() -> Result<(), AppError> {
    ADMISSION_BARRIER_READY.store(true, Ordering::SeqCst);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !ADMISSION_BARRIER_RELEASE.load(Ordering::SeqCst) {
        if std::time::Instant::now() >= deadline {
            return Err(AppError::runtime("admission barrier timeout"));
        }
        std::thread::yield_now();
    }
    Ok(())
}

fn fake_runner(
    prompt: &str,
    max_tokens: u32,
    _timeout_ms: u32,
    _team_id: &str,
) -> Result<subagent::WorkerGeneration, AppError> {
    let active = ACTIVE_RUNNERS.fetch_add(1, Ordering::SeqCst) + 1;
    MAX_ACTIVE_RUNNERS.fetch_max(active, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(60));
    ACTIVE_RUNNERS.fetch_sub(1, Ordering::SeqCst);
    let subagent_id = prompt_value(prompt, "subagent_id=");
    let parent_workflow_id = prompt_value(prompt, "parent_workflow_id=");
    let role = prompt_value(prompt, "role=");
    let evidence_ref = prompt
        .lines()
        .find_map(|line| line.strip_prefix("source pointer: "))
        .unwrap();
    Ok(subagent::WorkerGeneration {
        backend_event_id: format!("backend-{subagent_id}"),
        effective_max_tokens: max_tokens,
        response: format!(
            "{{\"schema_version\":1,\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"role\":\"{}\",\"status\":\"completed\",\"summary\":\"bounded result\",\"findings\":[],\"patch_proposal\":null,\"evidence_refs\":[\"{}\"],\"validation_gaps\":[],\"suggested_next_action\":\"reconcile team results\"}}",
            subagent_id, parent_workflow_id, role, evidence_ref,
        ),
    })
}

fn patch_runner(
    prompt: &str,
    max_tokens: u32,
    _timeout_ms: u32,
    _team_id: &str,
) -> Result<subagent::WorkerGeneration, AppError> {
    let subagent_id = prompt_value(prompt, "subagent_id=");
    let parent_workflow_id = prompt_value(prompt, "parent_workflow_id=");
    let role = prompt_value(prompt, "role=");
    let evidence_ref = prompt
        .lines()
        .find_map(|line| line.strip_prefix("source pointer: "))
        .unwrap();
    let source_hash = prompt
        .lines()
        .find_map(|line| line.strip_prefix("fingerprint: "))
        .unwrap();
    Ok(subagent::WorkerGeneration {
        backend_event_id: format!("backend-{subagent_id}"),
        effective_max_tokens: max_tokens,
        response: format!(
            "{{\"schema_version\":1,\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"role\":\"{}\",\"status\":\"completed\",\"summary\":\"bounded patch\",\"findings\":[],\"patch_proposal\":{{\"target_path\":\"src/main.rs\",\"source_hash\":\"{}\",\"find_text\":\"fn main() {{}}\",\"replacement_text\":\"fn main() {{ println!(\\\"ready\\\"); }}\"}},\"evidence_refs\":[\"{}\"],\"validation_gaps\":[],\"suggested_next_action\":\"reconcile team results\"}}",
            subagent_id, parent_workflow_id, role, source_hash, evidence_ref,
        ),
    })
}

fn one_worker_fails(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
    team_id: &str,
) -> Result<subagent::WorkerGeneration, AppError> {
    if prompt.contains("role=verifier") {
        return Err(AppError::runtime("injected worker failure"));
    }
    fake_runner(prompt, max_tokens, timeout_ms, team_id)
}

fn validation_gap_runner(
    prompt: &str,
    max_tokens: u32,
    _timeout_ms: u32,
    _team_id: &str,
) -> Result<subagent::WorkerGeneration, AppError> {
    let subagent_id = prompt_value(prompt, "subagent_id=");
    let parent_workflow_id = prompt_value(prompt, "parent_workflow_id=");
    let role = prompt_value(prompt, "role=");
    let evidence_ref = prompt
        .lines()
        .find_map(|line| line.strip_prefix("source pointer: "))
        .unwrap();
    Ok(subagent::WorkerGeneration {
        backend_event_id: format!("backend-{subagent_id}"),
        effective_max_tokens: max_tokens,
        response: format!(
            "{{\"schema_version\":1,\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"role\":\"{}\",\"status\":\"completed\",\"summary\":\"bounded result\",\"findings\":[],\"patch_proposal\":null,\"evidence_refs\":[\"{}\"],\"validation_gaps\":[\"verification not completed\"],\"suggested_next_action\":\"resolve verification gap\"}}",
            subagent_id, parent_workflow_id, role, evidence_ref,
        ),
    })
}

fn cancelling_runner(
    _prompt: &str,
    _max_tokens: u32,
    _timeout_ms: u32,
    team_id: &str,
) -> Result<subagent::WorkerGeneration, AppError> {
    if !CANCEL_STARTED.swap(true, Ordering::SeqCst) {
        team_state::cancel_report(team_id)?;
    }
    if team_state::cancellation_requested(team_id)? {
        CANCEL_OBSERVERS.fetch_add(1, Ordering::SeqCst);
        return Err(AppError::blocked("backend chat 취소됨"));
    }
    Err(AppError::runtime("team cancellation marker 누락"))
}

fn counting_runner(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
    team_id: &str,
) -> Result<subagent::WorkerGeneration, AppError> {
    RECOVERY_RUNNERS.fetch_add(1, Ordering::SeqCst);
    fake_runner(prompt, max_tokens, timeout_ms, team_id)
}

fn prompt_value<'a>(prompt: &'a str, marker: &str) -> &'a str {
    prompt
        .split(marker)
        .nth(1)
        .and_then(|value| value.split([',', '.']).next())
        .unwrap()
}

fn record_sample(pressure_status: &str) {
    observability::record_resource_sample(&observability::ResourceSampleMetric {
        resource_sample_id: format!("team-execution-{pressure_status}"),
        session_id: "session-team-execution".to_string(),
        backend_id: "llama.cpp".to_string(),
        pid: 4242,
        process_cpu_percent: Some(12.0),
        average_rss_bytes: Some(512 * 1024 * 1024),
        peak_rss_bytes: Some(512 * 1024 * 1024),
        disk_bytes: Some(2048),
        sample_count: 1,
        pressure_status: pressure_status.to_string(),
        recorded_at_ms: 1234,
    })
    .unwrap();
}

fn reset_runner_counters() {
    ACTIVE_RUNNERS.store(0, Ordering::SeqCst);
    MAX_ACTIVE_RUNNERS.store(0, Ordering::SeqCst);
}

fn admit_without_execute_transition() -> Vec<subagent::AdmittedTeamMember> {
    let identity = ledger::validated_current_identity().unwrap();
    let planned = team_state::load_state("team-execution").unwrap();
    let dispatch = team_state::advance_state(
        "team-execution",
        team_state::TeamStage::Dispatch,
        Some(2),
        Some("parallel"),
    )
    .unwrap();
    assert_eq!(planned.stage, team_state::TeamStage::Plan);
    let manifest = team_state::load_manifest("team-execution").unwrap();
    let admitted = subagent::admit_team_members(
        &dispatch.parent_workflow_id,
        dispatch.parent_revision,
        &dispatch.parent_artifact_hash,
        team_launches(&manifest),
    )
    .unwrap();
    for member in &admitted {
        append_worker_event(
            &identity,
            "team.worker.admitted",
            "team worker admitted",
            "team-execution",
            member.lane,
            &member.member_id,
            member.subagent_id(),
            "admitted",
            "none",
            "none",
        )
        .unwrap();
    }
    admitted
}

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
