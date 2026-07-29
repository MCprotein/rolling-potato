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
        .and_then(|value| value.split([',', '.', ';']).next())
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
