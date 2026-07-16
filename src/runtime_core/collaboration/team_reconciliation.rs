//! Team reconciliation binding, stage, ownership, and artifact policy.

use super::subagent_result::SubagentResultV1;
use super::team_execution::RuntimeIdentityBinding;
use super::team_state::{TeamManifestV1, TeamMemberV1, TeamStage, TeamStateV1};
use crate::foundation::error::AppError;
use crate::foundation::serialization::escape_string_content;
use std::collections::BTreeSet;

pub(crate) struct ReconciliationMemberBinding<'a> {
    pub lane: u32,
    pub member_id: &'a str,
    pub subagent_id: &'a str,
    pub result_artifact_id: &'a str,
    pub result_artifact_hash: &'a str,
    pub evidence_id: &'a str,
    pub evidence_hash: &'a str,
}

pub(crate) fn validate_reconciliation_binding(
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
            "team reconcile state/manifest/owner binding 불일치",
        ));
    }
    Ok(())
}

pub(crate) fn validate_reconciliation_stage(team: &TeamStateV1) -> Result<(), AppError> {
    if matches!(team.stage, TeamStage::Plan | TeamStage::Dispatch) {
        return Err(AppError::blocked(format!(
            "team reconcile stage 차단\n- team id: {}\n- current stage: {}\n- 이유: worker execution이 완료되지 않았습니다.",
            team.team_id,
            team.stage.as_str()
        )));
    }
    if matches!(team.stage, TeamStage::Failed | TeamStage::Cancelled) {
        return Err(AppError::blocked(format!(
            "team reconcile terminal state 차단\n- team id: {}\n- current stage: {}",
            team.team_id,
            team.stage.as_str()
        )));
    }
    Ok(())
}

pub(crate) fn validate_action_ownership<'a>(
    manifest: &'a TeamManifestV1,
    member: &'a TeamMemberV1,
    result: &'a SubagentResultV1,
) -> Result<(&'static str, &'a str, &'a str), AppError> {
    let Some(patch) = result.patch_proposal.as_ref() else {
        return Ok(("none", "none", "none"));
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
    if owners.len() != 1 || owners[0].lane != member.lane || owners[0].member_id != member.member_id
    {
        return Err(AppError::blocked(
            "team reconciliation action ownership 불일치",
        ));
    }
    Ok(("patch", &patch.target_path, &patch.source_hash))
}

pub(crate) fn parse_unique_evidence(value: &str) -> Result<Vec<String>, AppError> {
    let evidence = value
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if evidence.iter().collect::<BTreeSet<_>>().len() != evidence.len() {
        return Err(AppError::blocked(
            "team parent evidence 기존 목록에 duplicate가 있습니다.",
        ));
    }
    Ok(evidence)
}

pub(crate) fn render_reconciliation(
    team: &TeamStateV1,
    members: &[ReconciliationMemberBinding<'_>],
) -> String {
    let member_body = members
        .iter()
        .map(|member| {
            format!(
                "{{\"lane\":{},\"member_id\":\"{}\",\"subagent_id\":\"{}\",\"result_artifact_id\":\"{}\",\"result_artifact_hash\":\"{}\",\"evidence_id\":\"{}\",\"evidence_hash\":\"{}\"}}",
                member.lane,
                escape_string_content(member.member_id),
                escape_string_content(member.subagent_id),
                escape_string_content(member.result_artifact_id),
                member.result_artifact_hash,
                escape_string_content(member.evidence_id),
                member.evidence_hash,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema_version\":1,\"team_id\":\"{}\",\"manifest_hash\":\"{}\",\"parent_workflow_id\":\"{}\",\"parent_revision\":{},\"parent_artifact_hash\":\"{}\",\"members\":[{}]}}",
        escape_string_content(&team.team_id),
        team.manifest_hash,
        escape_string_content(&team.parent_workflow_id),
        team.parent_revision,
        team.parent_artifact_hash,
        member_body,
    )
}
