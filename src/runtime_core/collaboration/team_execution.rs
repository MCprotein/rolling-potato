//! Team execution binding, stage, mode, and action ownership policy.

use super::subagent::SubagentRecordV1;
use super::team_state::{TeamManifestV1, TeamStage, TeamStateV1};
use crate::foundation::error::AppError;
use crate::foundation::integrity;

pub(crate) struct RuntimeIdentityBinding<'a> {
    pub project_id: &'a str,
    pub session_id: &'a str,
}

pub(crate) struct ExecutionLaunchBinding<'a> {
    pub role: &'a str,
    pub task: &'a str,
    pub declared_tools: &'a [String],
    pub read_paths: &'a [String],
    pub write_paths: &'a [String],
    pub timeout_ms: u32,
    pub max_tokens: u32,
}

pub(crate) fn validate_execution_binding(
    identity: &RuntimeIdentityBinding<'_>,
    team: &TeamStateV1,
    manifest: &TeamManifestV1,
) -> Result<(), AppError> {
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
    Ok(())
}

pub(crate) fn validate_execution_stage(team: &TeamStateV1) -> Result<(), AppError> {
    if !matches!(
        team.stage,
        TeamStage::Plan | TeamStage::Dispatch | TeamStage::Execute
    ) {
        return Err(AppError::blocked(format!(
            "team execute stage 차단\n- team id: {}\n- current stage: {}",
            team.team_id,
            team.stage.as_str()
        )));
    }
    Ok(())
}

pub(crate) fn execution_mode(admitted_lanes: u32) -> &'static str {
    if admitted_lanes > 1 {
        "parallel"
    } else {
        "sequential"
    }
}

pub(crate) fn validate_action_owner(
    manifest: &TeamManifestV1,
    lane: u32,
    member_id: &str,
    target_path: &str,
) -> Result<(), AppError> {
    let owners = manifest
        .members
        .iter()
        .filter(|candidate| {
            candidate.write_paths.iter().any(|owner| {
                target_path == owner
                    || target_path
                        .strip_prefix(owner)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
        })
        .collect::<Vec<_>>();
    if owners.len() != 1 || owners[0].lane != lane || owners[0].member_id != member_id {
        return Err(AppError::blocked(format!(
            "team action-time ownership 차단\n- lane: {lane}\n- member: {member_id}\n- target: {target_path}"
        )));
    }
    Ok(())
}

pub(crate) fn detail_token<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

pub(crate) fn record_matches_team(
    identity: &RuntimeIdentityBinding<'_>,
    team: &TeamStateV1,
    launch: &ExecutionLaunchBinding<'_>,
    record: &SubagentRecordV1,
) -> bool {
    record.project_id == identity.project_id
        && record.session_id == identity.session_id
        && record.parent_workflow_id == team.parent_workflow_id
        && record.parent_revision == team.parent_revision
        && record.parent_artifact_hash == team.parent_artifact_hash
        && record.role.as_str() == launch.role
        && record.task_hash == integrity::sha256_text(launch.task.trim())
        && record.declared_tools == launch.declared_tools
        && record.read_paths == launch.read_paths
        && record.write_paths == launch.write_paths
        && record.timeout_ms == launch.timeout_ms
        && record.requested_max_tokens == launch.max_tokens
}

pub(crate) fn validate_completed_member_binding(
    member: &super::team_state::TeamMemberV1,
    record: &SubagentRecordV1,
) -> Result<(), AppError> {
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
    Ok(())
}
