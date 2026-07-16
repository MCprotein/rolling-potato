use crate::app::AppError;
use crate::{backend, ledger, observability, resource, subagent, team_state};

type TeamRunner = fn(&str, u32, u32, &str) -> Result<subagent::WorkerGeneration, AppError>;
type TeamPreflight = fn() -> Result<(), AppError>;

#[derive(Debug)]
struct MemberOutcome {
    lane: u32,
    member_id: String,
    subagent_id: String,
    result: Result<subagent::CompletedTeamMember, AppError>,
}

struct OwnedAction {
    target_path: String,
    source_hash: String,
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
    if team_state::cancellation_requested(team_id)? {
        return Err(AppError::blocked(format!(
            "team execute cancellation 차단\n- team id: {team_id}"
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
        .iter()
        .cloned()
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
        run_parallel(admitted, runner, team_id)?
    } else {
        run_sequential(admitted, runner, team_id)
    };
    let mut completed = Vec::new();
    let mut failures = Vec::new();
    for outcome in outcomes {
        match outcome.result {
            Ok(member) => {
                let owned_action = match enforce_action_ownership(&manifest, &member) {
                    Ok(owned_action) => owned_action,
                    Err(error) => {
                        append_worker_event(
                            &identity,
                            "team.worker.failed",
                            "team worker action ownership blocked",
                            team_id,
                            member.lane,
                            &member.member_id,
                            &member.record.subagent_id,
                            "ownership-blocked",
                            &member.record.result_artifact_id,
                            &member.record.evidence_id,
                        )?;
                        failures.push(format!("lane {}: {}", member.lane, error.message));
                        continue;
                    }
                };
                append_action_event(&identity, team_id, &member, owned_action.as_ref())?;
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
        let current = team_state::load_state(team_id)?;
        if current.stage == team_state::TeamStage::Cancelled {
            return Err(AppError::blocked(format!(
                "team execute cancelled\n- team id: {team_id}\n- completed lanes: {}",
                completed.len()
            )));
        }
        if !current.stage.is_terminal() {
            team_state::advance_state(team_id, team_state::TeamStage::Failed, None, None)?;
        }
        return Err(AppError::blocked(format!(
            "team execute worker failure\n- team id: {}\n- stage: failed\n- completed lanes: {}\n- failures: {}",
            team_id,
            completed.len(),
            failures.join(" | ")
        )));
    }
    if team_state::load_state(team_id)?.stage == team_state::TeamStage::Cancelled {
        return Err(AppError::blocked(format!(
            "team execute cancelled\n- team id: {team_id}\n- completed lanes: {}",
            completed.len()
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

fn enforce_action_ownership(
    manifest: &team_state::TeamManifestV1,
    completed: &subagent::CompletedTeamMember,
) -> Result<Option<OwnedAction>, AppError> {
    let member = manifest
        .members
        .iter()
        .find(|member| member.lane == completed.lane && member.member_id == completed.member_id)
        .ok_or_else(|| AppError::blocked("team completed member manifest binding 누락"))?;
    let record = &completed.record;
    if record.role.as_str() != member.role
        || record.task_hash != member.task_hash
        || record.declared_tools != member.tools
        || record.read_paths != member.read_paths
        || record.write_paths != member.write_paths
        || record.timeout_ms != member.timeout_ms
        || record.requested_max_tokens != member.max_tokens
    {
        return Err(AppError::blocked(
            "team completed member immutable launch binding 불일치",
        ));
    }
    let result = crate::subagent_result::load_completed_result(record)?;
    let Some(patch) = result.patch_proposal else {
        return Ok(None);
    };
    let owners = manifest
        .members
        .iter()
        .filter(|candidate| {
            candidate.write_paths.iter().any(|owner| {
                patch.target_path == *owner
                    || patch
                        .target_path
                        .strip_prefix(owner)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
        })
        .collect::<Vec<_>>();
    if owners.len() != 1
        || owners[0].lane != completed.lane
        || owners[0].member_id != completed.member_id
    {
        return Err(AppError::blocked(format!(
            "team action-time ownership 차단\n- lane: {}\n- member: {}\n- target: {}",
            completed.lane, completed.member_id, patch.target_path
        )));
    }
    Ok(Some(OwnedAction {
        target_path: patch.target_path,
        source_hash: patch.source_hash,
    }))
}

fn run_parallel(
    admitted: Vec<subagent::AdmittedTeamMember>,
    runner: TeamRunner,
    team_id: &str,
) -> Result<Vec<MemberOutcome>, AppError> {
    let prepared = subagent::prepare_team_members(admitted)?;
    let handles = prepared
        .into_iter()
        .map(|member| {
            let lane = member.lane;
            let member_id = member.member_id.clone();
            let subagent_id = member.subagent_id().to_string();
            let team_id = team_id.to_string();
            let handle = std::thread::spawn(move || {
                subagent::execute_prepared_team_member_with(
                    member,
                    |prompt, max_tokens, timeout| runner(prompt, max_tokens, timeout, &team_id),
                )
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
    team_id: &str,
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
                result: subagent::execute_admitted_team_member_with(
                    member,
                    |prompt, max_tokens, timeout| runner(prompt, max_tokens, timeout, team_id),
                ),
            }
        })
        .collect()
}

fn backend_runner(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
    team_id: &str,
) -> Result<subagent::WorkerGeneration, AppError> {
    let run = backend::chat_once_bounded_with_cancel(prompt, max_tokens, timeout_ms, || {
        team_state::cancellation_requested(team_id)
    })?;
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

fn append_action_event(
    identity: &ledger::RuntimeIdentity,
    team_id: &str,
    member: &subagent::CompletedTeamMember,
    action: Option<&OwnedAction>,
) -> Result<(), AppError> {
    let event = ledger::new_event_for(
        identity,
        "team.worker.action-owned",
        "team worker action ownership enforced",
        &format!(
            "team_id={} lane={} member_id={} subagent_id={} action={} target_path={} source_hash={}",
            team_id,
            member.lane,
            member.member_id,
            member.record.subagent_id,
            if action.is_some() { "patch" } else { "none" },
            action
                .map(|action| action.target_path.as_str())
                .unwrap_or("none"),
            action
                .map(|action| action.source_hash.as_str())
                .unwrap_or("none"),
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
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    static ACTIVE_RUNNERS: AtomicUsize = AtomicUsize::new(0);
    static MAX_ACTIVE_RUNNERS: AtomicUsize = AtomicUsize::new(0);
    static CANCEL_STARTED: AtomicBool = AtomicBool::new(false);
    static CANCEL_OBSERVERS: AtomicUsize = AtomicUsize::new(0);

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

        let report = crate::team_reconciliation::reconcile_report("team-execution").unwrap();
        let completed = team_state::load_state("team-execution").unwrap();
        let merged_parent = state::load_workflow(&parent.workflow_id).unwrap();
        let first_hash = merged_parent.artifact_hash.clone();
        let retry = crate::team_reconciliation::reconcile_report("team-execution").unwrap();
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

        let error = crate::team_reconciliation::reconcile_report("team-execution").unwrap_err();
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
}
