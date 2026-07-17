use crate::app::collaboration_adapter::{subagent, team_state};
use crate::app::inference_adapter::backend;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team::pressure_from_status;
use crate::runtime_core::collaboration::team_execution::{
    execution_mode, validate_execution_binding, validate_execution_stage, RuntimeIdentityBinding,
};
use crate::runtime_core::inference::resource;
use crate::adapters::filesystem::layout as paths;
use crate::adapters::filesystem::lease;

mod admission;
mod events;
#[cfg(test)]
use admission::team_launches;
use admission::{enforce_action_ownership, recover_or_admit_execution};
use events::{append_action_event, append_execution_blocked, append_worker_event};

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

enum ExecutionStart {
    Run(Vec<subagent::AdmittedTeamMember>),
    Completed(Vec<subagent::CompletedTeamMember>),
}

pub fn execute_report(team_id: &str) -> Result<String, AppError> {
    execute_with(team_id, backend::preflight_chat_ready, backend_runner)
}

fn execute_with(
    team_id: &str,
    preflight: TeamPreflight,
    runner: TeamRunner,
) -> Result<String, AppError> {
    let operation = lease::RecoverableLease::acquire(
        paths::project_team_operation_lock(team_id),
        "team operation",
    )?;
    let identity = ledger::validated_current_identity()?;
    let mut team = team_state::load_state(team_id)?;
    let manifest = team_state::load_manifest(team_id)?;
    validate_execution_binding(
        &RuntimeIdentityBinding {
            project_id: &identity.project_id,
            session_id: &identity.session_id,
        },
        &team,
        &manifest,
    )?;
    validate_execution_stage(&team)?;
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
            append_execution_blocked(&identity, &team, decision.reason)?;
            return Err(AppError::blocked(format!(
                "team execute resource admission 차단\n- team id: {}\n- pressure: {}\n- reason: {}",
                team.team_id,
                decision.pressure.as_str(),
                decision.reason
            )));
        }
        let execution_mode = execution_mode(decision.admitted_lanes);
        team = team_state::advance_state(
            team_id,
            team_state::TeamStage::Dispatch,
            Some(decision.admitted_lanes),
            Some(execution_mode),
        )?;
    }

    let start = recover_or_admit_execution(&identity, &team, &manifest)?;
    if matches!(&start, ExecutionStart::Run(_)) && team.stage == team_state::TeamStage::Dispatch {
        team = team_state::advance_state(team_id, team_state::TeamStage::Execute, None, None)?;
    }
    drop(operation);

    let outcomes = match start {
        ExecutionStart::Run(admitted) if team.execution_mode == "parallel" => {
            run_parallel(admitted, runner, team_id)?
        }
        ExecutionStart::Run(admitted) => run_sequential(admitted, runner, team_id),
        ExecutionStart::Completed(completed) => completed
            .into_iter()
            .map(|member| MemberOutcome {
                lane: member.lane,
                member_id: member.member_id.clone(),
                subagent_id: member.record.subagent_id.clone(),
                result: Ok(member),
            })
            .collect(),
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

#[cfg(test)]
#[path = "team_execution/tests.rs"]
mod tests;
