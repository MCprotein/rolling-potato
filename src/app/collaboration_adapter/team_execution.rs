use crate::app::collaboration_adapter::{subagent, team_state};
use crate::app::inference_adapter::backend;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team::pressure_from_status;
use crate::runtime_core::collaboration::team_execution::{
    detail_token, execution_mode, record_matches_team, validate_action_owner,
    validate_completed_member_binding, validate_execution_binding, validate_execution_stage,
    ExecutionLaunchBinding, RuntimeIdentityBinding,
};
use crate::runtime_core::inference::resource;
use crate::{adapters::filesystem::layout as paths, adapters::filesystem::lease};
use std::collections::BTreeMap;

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

fn recover_or_admit_execution(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
    manifest: &team_state::TeamManifestV1,
) -> Result<ExecutionStart, AppError> {
    let launches = team_launches(manifest);
    let bindings = admitted_worker_bindings(identity, team)?;
    if bindings.is_empty() && team.stage == team_state::TeamStage::Dispatch {
        let interrupted = subagent::records_for_parent(&team.parent_workflow_id)?
            .into_iter()
            .filter(|record| !record.status.is_terminal())
            .collect::<Vec<_>>();
        if interrupted
            .iter()
            .any(|record| record.status == subagent::SubagentStatus::Running)
        {
            return fail_interrupted_execution(
                team,
                interrupted
                    .iter()
                    .map(|record| record.subagent_id.clone())
                    .collect(),
                "unbound running worker cannot be replayed",
            );
        }
        if !interrupted.is_empty() {
            subagent::terminalize_interrupted_team_members(
                &interrupted
                    .iter()
                    .map(|record| record.subagent_id.clone())
                    .collect::<Vec<_>>(),
            )?;
        }
        let admitted = subagent::admit_team_members(
            &team.parent_workflow_id,
            team.parent_revision,
            &team.parent_artifact_hash,
            launches,
        )?;
        for member in &admitted {
            append_worker_event(
                identity,
                "team.worker.admitted",
                "team worker admitted",
                &team.team_id,
                member.lane,
                &member.member_id,
                member.subagent_id(),
                "admitted",
                "none",
                "none",
            )?;
        }
        return Ok(ExecutionStart::Run(admitted));
    }

    if bindings.len() != manifest.members.len() {
        let interrupted = subagent::records_for_parent(&team.parent_workflow_id)?
            .into_iter()
            .filter(|record| !record.status.is_terminal())
            .map(|record| record.subagent_id)
            .collect();
        return fail_interrupted_execution(
            team,
            interrupted,
            "partial team admission receipts cannot be reconstructed safely",
        );
    }

    let mut records = Vec::with_capacity(manifest.members.len());
    for (member, launch) in manifest.members.iter().zip(launches.iter()) {
        let (event_member_id, subagent_id) = bindings
            .get(&member.lane)
            .ok_or_else(|| AppError::blocked("team execute admitted lane binding 누락"))?;
        if event_member_id != &member.member_id {
            return fail_interrupted_execution(
                team,
                Vec::new(),
                "team admission member binding mismatch",
            );
        }
        let record = subagent::load_record(subagent_id)?;
        if !record_matches_team(
            &RuntimeIdentityBinding {
                project_id: &identity.project_id,
                session_id: &identity.session_id,
            },
            team,
            &ExecutionLaunchBinding {
                role: &launch.role,
                task: &launch.task,
                declared_tools: &launch.declared_tools,
                read_paths: &launch.read_paths,
                write_paths: &launch.write_paths,
                timeout_ms: launch.timeout_ms,
                max_tokens: launch.max_tokens,
            },
            &record,
        ) {
            return fail_interrupted_execution(
                team,
                vec![record.subagent_id],
                "team admission immutable binding mismatch",
            );
        }
        records.push((launch.clone(), record));
    }

    if team.stage == team_state::TeamStage::Execute
        && records
            .iter()
            .all(|(_, record)| record.status == subagent::SubagentStatus::Completed)
    {
        let completed = records
            .into_iter()
            .map(|(launch, record)| {
                let result =
                    crate::app::collaboration_adapter::subagent_result::load_completed_result(
                        &record,
                    )?;
                Ok(subagent::CompletedTeamMember {
                    lane: launch.lane,
                    member_id: launch.member_id,
                    record,
                    summary: result.summary,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        return Ok(ExecutionStart::Completed(completed));
    }

    if records
        .iter()
        .all(|(_, record)| record.status == subagent::SubagentStatus::Admitted)
    {
        let admitted = records
            .into_iter()
            .map(|(launch, record)| {
                subagent::resume_admitted_team_member(launch, &record.subagent_id)
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(ExecutionStart::Run(admitted));
    }

    fail_interrupted_execution(
        team,
        records
            .into_iter()
            .filter(|(_, record)| !record.status.is_terminal())
            .map(|(_, record)| record.subagent_id)
            .collect(),
        "running or partial worker result cannot be replayed safely",
    )
}

fn team_launches(manifest: &team_state::TeamManifestV1) -> Vec<subagent::TeamMemberLaunch> {
    manifest
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
        .collect()
}

fn admitted_worker_bindings(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
) -> Result<BTreeMap<u32, (String, String)>, AppError> {
    let mut bindings = BTreeMap::new();
    for event in ledger::read_runtime_events()?.into_iter().filter(|event| {
        event.project_id == identity.project_id
            && event.session_id == identity.session_id
            && event.event_type == "team.worker.admitted"
            && detail_token(&event.details, "team_id") == Some(team.team_id.as_str())
    }) {
        let lane = detail_token(&event.details, "lane")
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| AppError::blocked("team execute admitted event lane binding 오류"))?;
        let binding = (
            detail_token(&event.details, "member_id")
                .ok_or_else(|| AppError::blocked("team execute admitted member binding 누락"))?
                .to_string(),
            detail_token(&event.details, "subagent_id")
                .ok_or_else(|| AppError::blocked("team execute admitted subagent binding 누락"))?
                .to_string(),
        );
        if bindings
            .insert(lane, binding.clone())
            .is_some_and(|existing| existing != binding)
        {
            return Err(AppError::blocked(
                "team execute admitted lane에 conflicting worker binding이 있습니다.",
            ));
        }
    }
    Ok(bindings)
}

fn fail_interrupted_execution(
    team: &team_state::TeamStateV1,
    subagent_ids: Vec<String>,
    reason: &str,
) -> Result<ExecutionStart, AppError> {
    if !subagent_ids.is_empty() {
        subagent::terminalize_interrupted_team_members(&subagent_ids)?;
    }
    let current = team_state::load_state(&team.team_id)?;
    if !current.stage.is_terminal() {
        team_state::advance_state(&team.team_id, team_state::TeamStage::Failed, None, None)?;
    }
    Err(AppError::blocked(format!(
        "team execute interrupted recovery\n- team id: {}\n- stage: failed\n- reason: {reason}",
        team.team_id
    )))
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
    validate_completed_member_binding(member, record)?;
    let result = crate::app::collaboration_adapter::subagent_result::load_completed_result(record)?;
    let Some(patch) = result.patch_proposal else {
        return Ok(None);
    };
    validate_action_owner(
        manifest,
        completed.lane,
        &completed.member_id,
        &patch.target_path,
    )?;
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
    let details = format!(
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
    );
    if has_exact_event(identity, "team.worker.action-owned", &details)? {
        return Ok(());
    }
    let event = ledger::new_event_for(
        identity,
        "team.worker.action-owned",
        "team worker action ownership enforced",
        &details,
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
    let details = format!(
        "team_id={} lane={} member_id={} subagent_id={} status={} result_artifact_id={} evidence_id={}",
        team_id, lane, member_id, subagent_id, status, result_artifact_id, evidence_id,
    );
    if has_exact_event(identity, event_type, &details)? {
        return Ok(());
    }
    let event = ledger::new_event_for(identity, event_type, summary, &details);
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

fn has_exact_event(
    identity: &ledger::RuntimeIdentity,
    event_type: &str,
    details: &str,
) -> Result<bool, AppError> {
    Ok(ledger::read_runtime_events()?.iter().any(|event| {
        event.project_id == identity.project_id
            && event.session_id == identity.session_id
            && event.event_type == event_type
            && event.details == details
    }))
}

#[cfg(test)]
#[path = "team_execution/tests.rs"]
mod tests;
