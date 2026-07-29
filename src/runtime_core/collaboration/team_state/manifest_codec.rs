use std::collections::{BTreeMap, BTreeSet};

use super::super::subagent;
use super::{
    validate_id, TeamManifestV1, TeamMemberV1, MAX_MANIFEST_BYTES, MAX_MEMBER_ID_BYTES,
    MAX_TEAM_ID_BYTES, MAX_TEAM_LANES, TEAM_SCHEMA_VERSION,
};
use crate::foundation::error::AppError;
use crate::foundation::integrity;
use crate::foundation::serialization as strict_json;

const MANIFEST_KEYS: &[&str] = &[
    "schema_version",
    "team_id",
    "parent_workflow_id",
    "members",
    "write_policy",
    "merge_policy",
    "stop_gate",
];
const MEMBER_KEYS: &[&str] = &[
    "lane",
    "id",
    "role",
    "task",
    "tools",
    "read_paths",
    "write_paths",
    "timeout_ms",
    "max_tokens",
];

pub fn parse_manifest(body: &str) -> Result<TeamManifestV1, AppError> {
    if body.is_empty() || body.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(AppError::blocked("team manifest 크기 상한 위반"));
    }
    let value = strict_json::parse_value(body, "team manifest")?;
    if strict_json::render_compact(&value) != body {
        return Err(AppError::blocked(
            "team manifest는 canonical compact JSON이어야 합니다.",
        ));
    }
    let strict_json::Value::Object(object) = value else {
        return Err(AppError::blocked("team manifest root는 object여야 합니다."));
    };
    require_keys(&object, MANIFEST_KEYS, "team manifest")?;
    if strict_json::number(&object, "schema_version", "team manifest")? != TEAM_SCHEMA_VERSION {
        return Err(AppError::blocked(
            "지원하지 않는 team manifest schema입니다.",
        ));
    }
    let team_id = strict_json::string(&object, "team_id", "team manifest")?;
    validate_id(&team_id, "team id", MAX_TEAM_ID_BYTES)?;
    let parent_workflow_id = strict_json::string(&object, "parent_workflow_id", "team manifest")?;
    if !parent_workflow_id.starts_with("workflow-") {
        return Err(AppError::blocked(
            "team manifest parent workflow id 형식 오류",
        ));
    }
    let members = parse_members(&object)?;
    let write_policy = strict_json::string(&object, "write_policy", "team manifest")?;
    let merge_policy = strict_json::string(&object, "merge_policy", "team manifest")?;
    let stop_gate = strict_json::string(&object, "stop_gate", "team manifest")?;
    if write_policy != "single_writer"
        || merge_policy != "runtime_owned"
        || stop_gate != "evidence_required"
    {
        return Err(AppError::blocked(
            "team manifest policy는 single_writer/runtime_owned/evidence_required 고정값이어야 합니다.",
        ));
    }
    validate_member_set(&members)?;
    Ok(TeamManifestV1 {
        team_id,
        parent_workflow_id,
        members,
        write_policy,
        merge_policy,
        stop_gate,
        artifact_hash: integrity::sha256_text(body),
        canonical_body: body.to_string(),
    })
}

fn parse_members(object: &strict_json::Object) -> Result<Vec<TeamMemberV1>, AppError> {
    let Some(strict_json::Value::Array(values)) = object.get("members") else {
        return Err(AppError::blocked(
            "team manifest members는 array여야 합니다.",
        ));
    };
    if values.is_empty() || values.len() > MAX_TEAM_LANES {
        return Err(AppError::blocked(format!(
            "team manifest member 수는 1..={MAX_TEAM_LANES} 범위여야 합니다."
        )));
    }
    values
        .iter()
        .map(|value| {
            let strict_json::Value::Object(member) = value else {
                return Err(AppError::blocked("team member는 object여야 합니다."));
            };
            require_keys(member, MEMBER_KEYS, "team member")?;
            let lane = u32::try_from(strict_json::number(member, "lane", "team member")?)
                .map_err(|_| AppError::blocked("team member lane 범위 오류"))?;
            let member_id = strict_json::string(member, "id", "team member")?;
            validate_id(&member_id, "team member id", MAX_MEMBER_ID_BYTES)?;
            let role = strict_json::string(member, "role", "team member")?;
            let task = strict_json::string(member, "task", "team member")?;
            let tools = string_array(member, "tools", "team member")?;
            let read_paths = string_array(member, "read_paths", "team member")?;
            let write_paths = string_array(member, "write_paths", "team member")?;
            let timeout_ms =
                u32::try_from(strict_json::number(member, "timeout_ms", "team member")?)
                    .map_err(|_| AppError::blocked("team member timeout 범위 오류"))?;
            let max_tokens =
                u32::try_from(strict_json::number(member, "max_tokens", "team member")?)
                    .map_err(|_| AppError::blocked("team member max token 범위 오류"))?;
            let launch = subagent::validate_launch(
                &role,
                &task,
                &tools,
                &read_paths,
                &write_paths,
                Some(timeout_ms),
                Some(max_tokens),
            )?;
            Ok(TeamMemberV1 {
                lane,
                member_id,
                role: launch.role.as_str().to_string(),
                task,
                task_hash: launch.task_hash,
                tools: launch.declared_tools,
                read_paths: launch.read_paths,
                write_paths: launch.write_paths,
                timeout_ms: launch.timeout_ms,
                max_tokens: launch.requested_max_tokens,
            })
        })
        .collect()
}

fn validate_member_set(members: &[TeamMemberV1]) -> Result<(), AppError> {
    let mut ids = BTreeSet::new();
    let mut ownership = BTreeMap::<String, u32>::new();
    for (index, member) in members.iter().enumerate() {
        let expected_lane = (index + 1) as u32;
        if member.lane != expected_lane {
            return Err(AppError::blocked(format!(
                "team member lane은 1부터 순서대로 선언해야 합니다: expected={expected_lane} actual={}",
                member.lane
            )));
        }
        if !ids.insert(member.member_id.as_str()) {
            return Err(AppError::blocked("team member id 중복 차단"));
        }
        for path in &member.write_paths {
            if let Some((owned_path, owner)) = ownership
                .iter()
                .find(|(owned_path, _)| ownership_paths_overlap(owned_path, path))
            {
                return Err(AppError::blocked(format!(
                    "team manifest cross-lane ownership 충돌\n- paths: {owned_path}, {path}\n- lanes: {owner}, {}",
                    member.lane
                )));
            }
            ownership.insert(path.clone(), member.lane);
        }
    }
    Ok(())
}

fn ownership_paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('/'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn require_keys(
    object: &strict_json::Object,
    expected: &[&str],
    context: &str,
) -> Result<(), AppError> {
    let actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    if actual != expected {
        return Err(AppError::blocked(format!(
            "{context} key order/schema 불일치"
        )));
    }
    Ok(())
}

fn string_array(
    object: &strict_json::Object,
    key: &str,
    context: &str,
) -> Result<Vec<String>, AppError> {
    let Some(strict_json::Value::Array(values)) = object.get(key) else {
        return Err(AppError::blocked(format!("{context}: {key} array 필요")));
    };
    values
        .iter()
        .map(|value| match value {
            strict_json::Value::String(value) => Ok(value.clone()),
            _ => Err(AppError::blocked(format!(
                "{context}: {key} item type 오류"
            ))),
        })
        .collect()
}
