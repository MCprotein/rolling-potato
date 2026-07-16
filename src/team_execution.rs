use crate::app::AppError;
use crate::{backend, ledger, observability, resource, subagent, team_state};

type TeamRunner = fn(&str, u32, u32) -> Result<subagent::WorkerGeneration, AppError>;
type TeamPreflight = fn() -> Result<(), AppError>;

#[derive(Debug)]
struct MemberOutcome {
    lane: u32,
    member_id: String,
    subagent_id: String,
    result: Result<subagent::CompletedTeamMember, AppError>,
}

pub fn execute_report(team_id: &str) -> Result<String, AppError> {
    execute_with(team_id, backend::preflight_chat_ready, backend_runner)
}

fn execute_with(
    team_id: &str,
    preflight: TeamPreflight,
    runner: TeamRunner,
) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let mut team = team_state::load_state(team_id)?;
    let manifest = team_state::load_manifest(team_id)?;
    if team.manifest_hash != manifest.artifact_hash
        || team.parent_workflow_id != manifest.parent_workflow_id
        || team.member_count != manifest.members.len() as u32
        || team.project_id != identity.project_id
        || team.session_id != identity.session_id
    {
        return Err(AppError::blocked(
            "team execute state/manifest/owner binding 불일치",
        ));
    }
    if !matches!(
        team.stage,
        team_state::TeamStage::Plan | team_state::TeamStage::Dispatch
    ) {
        return Err(AppError::blocked(format!(
            "team execute stage 차단\n- team id: {}\n- current stage: {}",
            team.team_id,
            team.stage.as_str()
        )));
    }

    preflight()?;
    if team.stage == team_state::TeamStage::Plan {
        let sample = observability::latest_resource_sample()?;
        let pressure = sample
            .as_ref()
            .map(|sample| pressure_from_status(&sample.pressure_status))
            .unwrap_or(resource::ResourcePressure::Unknown);
        let decision = resource::team_lane_decision(pressure, team.requested_lanes);
        if decision.is_blocked() {
            append_execution_blocked(&identity, &team, &decision.reason)?;
            return Err(AppError::blocked(format!(
                "team execute resource admission 차단\n- team id: {}\n- pressure: {}\n- reason: {}",
                team.team_id,
                decision.pressure.as_str(),
                decision.reason
            )));
        }
        let execution_mode = if decision.admitted_lanes > 1 {
            "parallel"
        } else {
            "sequential"
        };
        team = team_state::advance_state(
            team_id,
            team_state::TeamStage::Dispatch,
            Some(decision.admitted_lanes),
            Some(execution_mode),
        )?;
    }

    let launches = manifest
        .members
        .into_iter()
        .map(|member| subagent::TeamMemberLaunch {
            lane: member.lane,
            member_id: member.member_id,
            role: member.role,
            task: member.task,
            declared_tools: member.tools,
            read_paths: member.read_paths,
            write_paths: member.write_paths,
            timeout_ms: member.timeout_ms,
            max_tokens: member.max_tokens,
        })
        .collect::<Vec<_>>();
    let admitted = subagent::admit_team_members(
        &team.parent_workflow_id,
        team.parent_revision,
        &team.parent_artifact_hash,
        launches,
    )?;
    for member in &admitted {
        append_worker_event(
            &identity,
            "team.worker.admitted",
            "team worker admitted",
            team_id,
            member.lane,
            &member.member_id,
            member.subagent_id(),
            "admitted",
            "none",
            "none",
        )?;
    }
    team = team_state::advance_state(team_id, team_state::TeamStage::Execute, None, None)?;

    let outcomes = if team.execution_mode == "parallel" {
        run_parallel(admitted, runner)?
    } else {
        run_sequential(admitted, runner)
    };
    let mut completed = Vec::new();
    let mut failures = Vec::new();
    for outcome in outcomes {
        match outcome.result {
            Ok(member) => {
                append_worker_event(
                    &identity,
                    "team.worker.completed",
                    "team worker completed",
                    team_id,
                    member.lane,
                    &member.member_id,
                    &member.record.subagent_id,
                    member.record.status.as_str(),
                    &member.record.result_artifact_id,
                    &member.record.evidence_id,
                )?;
                completed.push(member);
            }
            Err(error) => {
                append_worker_event(
                    &identity,
                    "team.worker.failed",
                    "team worker failed",
                    team_id,
                    outcome.lane,
                    &outcome.member_id,
                    &outcome.subagent_id,
                    "failed",
                    "none",
                    "none",
                )?;
                failures.push(format!("lane {}: {}", outcome.lane, error.message));
            }
        }
    }
    if !failures.is_empty() {
        return Err(AppError::blocked(format!(
            "team execute worker failure\n- team id: {}\n- completed lanes: {}\n- failures: {}",
            team_id,
            completed.len(),
            failures.join(" | ")
        )));
    }
    completed.sort_by_key(|member| member.lane);
    Ok(format!(
        "team execute\n- status: workers-completed\n- team id: {}\n- stage: {}\n- execution mode: {}\n- requested lanes: {}\n- admitted lanes: {}\n- completed members: {}\n- worker ids: {}\n- result artifacts: {}\n- evidence artifacts: {}\n- boundary: worker results are stored and unmerged; parent evidence reconciliation and completion gates run in later team stages.",
        team.team_id,
        team.stage.as_str(),
        team.execution_mode,
        team.requested_lanes,
        team.admitted_lanes,
        completed.len(),
        completed
            .iter()
            .map(|member| member.record.subagent_id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        completed
            .iter()
            .map(|member| member.record.result_artifact_id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        completed
            .iter()
            .map(|member| member.record.evidence_id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    ))
}

fn run_parallel(
    admitted: Vec<subagent::AdmittedTeamMember>,
    runner: TeamRunner,
) -> Result<Vec<MemberOutcome>, AppError> {
    let prepared = subagent::prepare_team_members(admitted)?;
    let handles = prepared
        .into_iter()
        .map(|member| {
            let lane = member.lane;
            let member_id = member.member_id.clone();
            let subagent_id = member.subagent_id().to_string();
            let handle = std::thread::spawn(move || {
                subagent::execute_prepared_team_member_with(member, runner)
            });
            (lane, member_id, subagent_id, handle)
        })
        .collect::<Vec<_>>();
    Ok(handles
        .into_iter()
        .map(|(lane, member_id, subagent_id, handle)| MemberOutcome {
            lane,
            member_id,
            subagent_id,
            result: handle
                .join()
                .unwrap_or_else(|_| Err(AppError::runtime("team worker thread panic"))),
        })
        .collect())
}

fn run_sequential(
    admitted: Vec<subagent::AdmittedTeamMember>,
    runner: TeamRunner,
) -> Vec<MemberOutcome> {
    admitted
        .into_iter()
        .map(|member| {
            let lane = member.lane;
            let member_id = member.member_id.clone();
            let subagent_id = member.subagent_id().to_string();
            MemberOutcome {
                lane,
                member_id,
                subagent_id,
                result: subagent::execute_admitted_team_member_with(member, runner),
            }
        })
        .collect()
}

fn backend_runner(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
) -> Result<subagent::WorkerGeneration, AppError> {
    let run = backend::chat_once_bounded(prompt, max_tokens, timeout_ms)?;
    Ok(subagent::WorkerGeneration {
        backend_event_id: run.ledger_event,
        effective_max_tokens: run.effective_max_tokens,
        response: run.response,
    })
}

fn pressure_from_status(value: &str) -> resource::ResourcePressure {
    match value {
        "normal" => resource::ResourcePressure::Normal,
        "degraded" => resource::ResourcePressure::Degraded,
        "critical" => resource::ResourcePressure::Critical,
        _ => resource::ResourcePressure::Unknown,
    }
}

fn append_execution_blocked(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
    reason: &str,
) -> Result<(), AppError> {
    let event = ledger::new_event_for(
        identity,
        "team.execution.blocked",
        "team execution resource blocked",
        &format!(
            "team_id={} stage={} requested_lanes={} reason={}",
            team.team_id,
            team.stage.as_str(),
            team.requested_lanes,
            ledger::redact_text(reason),
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

#[allow(clippy::too_many_arguments)]
fn append_worker_event(
    identity: &ledger::RuntimeIdentity,
    event_type: &str,
    summary: &str,
    team_id: &str,
    lane: u32,
    member_id: &str,
    subagent_id: &str,
    status: &str,
    result_artifact_id: &str,
    evidence_id: &str,
) -> Result<(), AppError> {
    let event = ledger::new_event_for(
        identity,
        event_type,
        summary,
        &format!(
            "team_id={} lane={} member_id={} subagent_id={} status={} result_artifact_id={} evidence_id={}",
            team_id,
            lane,
            member_id,
            subagent_id,
            status,
            result_artifact_id,
            evidence_id,
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{paths, state};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    static ACTIVE_RUNNERS: AtomicUsize = AtomicUsize::new(0);
    static MAX_ACTIVE_RUNNERS: AtomicUsize = AtomicUsize::new(0);

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

    fn fake_preflight() -> Result<(), AppError> {
        Ok(())
    }

    fn fake_runner(
        prompt: &str,
        max_tokens: u32,
        _timeout_ms: u32,
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
}
