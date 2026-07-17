use super::events::append_worker_event;
use super::{ledger, subagent, team_state, AppError, ExecutionStart, OwnedAction};
use crate::runtime_core::collaboration::team_execution::{
    detail_token, record_matches_team, validate_action_owner, validate_completed_member_binding,
    ExecutionLaunchBinding, RuntimeIdentityBinding,
};
use std::collections::BTreeMap;

pub(super) fn recover_or_admit_execution(
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

pub(super) fn team_launches(
    manifest: &team_state::TeamManifestV1,
) -> Vec<subagent::TeamMemberLaunch> {
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

pub(super) fn enforce_action_ownership(
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
