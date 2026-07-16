//! Team manifest DTOs, persisted state DTO, and stage transition policy.

use super::subagent;
use crate::foundation::error::AppError;
use crate::foundation::integrity;
use crate::foundation::serialization as strict_json;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const TEAM_SCHEMA_VERSION: u64 = 1;
pub(crate) const MAX_TEAM_LANES: usize = 4;
pub(crate) const MAX_TEAM_ID_BYTES: usize = 64;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 65_536;
pub(crate) const MAX_STATE_BYTES: u64 = 65_536;
pub(crate) const MAX_STATE_REVISIONS: u64 = 64;

const MAX_MEMBER_ID_BYTES: usize = 64;
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
const STATE_KEYS: &[&str] = &[
    "schema_version",
    "team_id",
    "revision",
    "previous_hash",
    "artifact_hash",
    "manifest_hash",
    "project_id",
    "session_id",
    "parent_workflow_id",
    "parent_revision",
    "parent_artifact_hash",
    "stage",
    "status",
    "requested_lanes",
    "admitted_lanes",
    "execution_mode",
    "member_count",
    "created_at_ms",
    "updated_at_ms",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMemberV1 {
    pub lane: u32,
    pub member_id: String,
    pub role: String,
    pub task: String,
    pub task_hash: String,
    pub tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamManifestV1 {
    pub team_id: String,
    pub parent_workflow_id: String,
    pub members: Vec<TeamMemberV1>,
    pub write_policy: String,
    pub merge_policy: String,
    pub stop_gate: String,
    pub artifact_hash: String,
    pub(crate) canonical_body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamStage {
    Plan,
    Dispatch,
    Execute,
    Review,
    Verify,
    Merge,
    Report,
    Complete,
    Failed,
    Cancelled,
}

impl TeamStage {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "team-plan" => Some(Self::Plan),
            "team-dispatch" => Some(Self::Dispatch),
            "team-exec" => Some(Self::Execute),
            "team-review" => Some(Self::Review),
            "team-verify" => Some(Self::Verify),
            "team-merge" => Some(Self::Merge),
            "team-report" => Some(Self::Report),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "team-plan",
            Self::Dispatch => "team-dispatch",
            Self::Execute => "team-exec",
            Self::Review => "team-review",
            Self::Verify => "team-verify",
            Self::Merge => "team-merge",
            Self::Report => "team-report",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled)
    }

    pub(crate) fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Plan, Self::Dispatch)
                | (Self::Dispatch, Self::Execute)
                | (Self::Execute, Self::Review)
                | (Self::Review, Self::Verify)
                | (Self::Verify, Self::Merge)
                | (Self::Merge, Self::Report)
                | (Self::Report, Self::Complete)
        ) || (!self.is_terminal() && matches!(next, Self::Failed | Self::Cancelled))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamStateV1 {
    pub team_id: String,
    pub revision: u64,
    pub previous_hash: String,
    pub artifact_hash: String,
    pub manifest_hash: String,
    pub project_id: String,
    pub session_id: String,
    pub parent_workflow_id: String,
    pub parent_revision: u64,
    pub parent_artifact_hash: String,
    pub stage: TeamStage,
    pub status: String,
    pub requested_lanes: u32,
    pub admitted_lanes: u32,
    pub execution_mode: String,
    pub member_count: u32,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

impl TeamStateV1 {
    pub(crate) fn transition_to_at(
        &mut self,
        next: TeamStage,
        admitted_lanes: Option<u32>,
        execution_mode: Option<&str>,
        updated_at_ms: u128,
    ) -> Result<(), AppError> {
        if !self.stage.permits(next) {
            return Err(AppError::blocked(format!(
                "team stage 전이 차단\n- current: {}\n- next: {}",
                self.stage.as_str(),
                next.as_str()
            )));
        }
        if next == TeamStage::Dispatch {
            let admitted_lanes = admitted_lanes.ok_or_else(|| {
                AppError::blocked("team dispatch stage에는 admitted lane 수가 필요합니다.")
            })?;
            if admitted_lanes == 0 || admitted_lanes > self.requested_lanes {
                return Err(AppError::blocked(
                    "team dispatch admitted lane binding이 요청 범위를 벗어났습니다.",
                ));
            }
            let execution_mode = execution_mode.unwrap_or("");
            if !matches!(execution_mode, "parallel" | "sequential") {
                return Err(AppError::blocked(
                    "team dispatch execution mode는 parallel 또는 sequential이어야 합니다.",
                ));
            }
            self.admitted_lanes = admitted_lanes;
            self.execution_mode = execution_mode.to_string();
        } else if admitted_lanes.is_some() || execution_mode.is_some() {
            return Err(AppError::blocked(
                "team dispatch 외 stage에서 admission binding을 변경할 수 없습니다.",
            ));
        }
        self.stage = next;
        self.status = match next {
            TeamStage::Complete => "completed",
            TeamStage::Failed => "failed",
            TeamStage::Cancelled => "cancelled",
            _ => "active",
        }
        .to_string();
        self.updated_at_ms = updated_at_ms;
        Ok(())
    }
}

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

pub(crate) fn render_payload(record: &TeamStateV1) -> String {
    format!(
        "{{\"schema_version\":{TEAM_SCHEMA_VERSION},\"team_id\":\"{}\",\"revision\":{},\"previous_hash\":\"{}\",\"manifest_hash\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"parent_revision\":{},\"parent_artifact_hash\":\"{}\",\"stage\":\"{}\",\"status\":\"{}\",\"requested_lanes\":{},\"admitted_lanes\":{},\"execution_mode\":\"{}\",\"member_count\":{},\"created_at_ms\":{},\"updated_at_ms\":{}}}",
        strict_json::escape_string_content(&record.team_id),
        record.revision,
        strict_json::escape_string_content(&record.previous_hash),
        strict_json::escape_string_content(&record.manifest_hash),
        strict_json::escape_string_content(&record.project_id),
        strict_json::escape_string_content(&record.session_id),
        strict_json::escape_string_content(&record.parent_workflow_id),
        record.parent_revision,
        strict_json::escape_string_content(&record.parent_artifact_hash),
        record.stage.as_str(),
        record.status,
        record.requested_lanes,
        record.admitted_lanes,
        record.execution_mode,
        record.member_count,
        record.created_at_ms,
        record.updated_at_ms,
    )
}

pub(crate) fn render_state(record: &TeamStateV1) -> String {
    format!(
        "{{\"schema_version\":{TEAM_SCHEMA_VERSION},\"team_id\":\"{}\",\"revision\":{},\"previous_hash\":\"{}\",\"artifact_hash\":\"{}\",\"manifest_hash\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"parent_revision\":{},\"parent_artifact_hash\":\"{}\",\"stage\":\"{}\",\"status\":\"{}\",\"requested_lanes\":{},\"admitted_lanes\":{},\"execution_mode\":\"{}\",\"member_count\":{},\"created_at_ms\":{},\"updated_at_ms\":{}}}",
        strict_json::escape_string_content(&record.team_id),
        record.revision,
        strict_json::escape_string_content(&record.previous_hash),
        strict_json::escape_string_content(&record.artifact_hash),
        strict_json::escape_string_content(&record.manifest_hash),
        strict_json::escape_string_content(&record.project_id),
        strict_json::escape_string_content(&record.session_id),
        strict_json::escape_string_content(&record.parent_workflow_id),
        record.parent_revision,
        strict_json::escape_string_content(&record.parent_artifact_hash),
        record.stage.as_str(),
        record.status,
        record.requested_lanes,
        record.admitted_lanes,
        record.execution_mode,
        record.member_count,
        record.created_at_ms,
        record.updated_at_ms,
    )
}

pub(crate) fn parse_state(body: &str) -> Result<TeamStateV1, AppError> {
    let object = strict_json::parse_object_exact_order(body, STATE_KEYS, "team state")?;
    if strict_json::number(&object, "schema_version", "team state")? != TEAM_SCHEMA_VERSION {
        return Err(AppError::blocked("지원하지 않는 team state schema"));
    }
    let stage_value = strict_json::string(&object, "stage", "team state")?;
    let stage = TeamStage::parse(&stage_value)
        .ok_or_else(|| AppError::blocked("team state stage 형식 오류"))?;
    let record = TeamStateV1 {
        team_id: strict_json::string(&object, "team_id", "team state")?,
        revision: strict_json::number(&object, "revision", "team state")?,
        previous_hash: strict_json::string(&object, "previous_hash", "team state")?,
        artifact_hash: strict_json::string(&object, "artifact_hash", "team state")?,
        manifest_hash: strict_json::string(&object, "manifest_hash", "team state")?,
        project_id: strict_json::string(&object, "project_id", "team state")?,
        session_id: strict_json::string(&object, "session_id", "team state")?,
        parent_workflow_id: strict_json::string(&object, "parent_workflow_id", "team state")?,
        parent_revision: strict_json::number(&object, "parent_revision", "team state")?,
        parent_artifact_hash: strict_json::string(&object, "parent_artifact_hash", "team state")?,
        stage,
        status: strict_json::string(&object, "status", "team state")?,
        requested_lanes: u32::try_from(strict_json::number(
            &object,
            "requested_lanes",
            "team state",
        )?)
        .map_err(|_| AppError::blocked("team state requested lane 범위 오류"))?,
        admitted_lanes: u32::try_from(strict_json::number(
            &object,
            "admitted_lanes",
            "team state",
        )?)
        .map_err(|_| AppError::blocked("team state admitted lane 범위 오류"))?,
        execution_mode: strict_json::string(&object, "execution_mode", "team state")?,
        member_count: u32::try_from(strict_json::number(&object, "member_count", "team state")?)
            .map_err(|_| AppError::blocked("team state member count 범위 오류"))?,
        created_at_ms: strict_json::number_u128(&object, "created_at_ms", "team state")?,
        updated_at_ms: strict_json::number_u128(&object, "updated_at_ms", "team state")?,
    };
    validate_state(&record, true)?;
    Ok(record)
}

pub(crate) fn validate_state(record: &TeamStateV1, installed: bool) -> Result<(), AppError> {
    validate_id(&record.team_id, "team id", MAX_TEAM_ID_BYTES)?;
    if record.revision == 0
        || record.revision > MAX_STATE_REVISIONS
        || !is_sha256(&record.manifest_hash)
        || record.project_id.is_empty()
        || record.session_id.is_empty()
        || !record.parent_workflow_id.starts_with("workflow-")
        || record.parent_revision == 0
        || !is_sha256(&record.parent_artifact_hash)
        || record.requested_lanes == 0
        || record.requested_lanes as usize > MAX_TEAM_LANES
        || record.member_count != record.requested_lanes
        || record.admitted_lanes > record.requested_lanes
        || record.created_at_ms == 0
        || record.updated_at_ms < record.created_at_ms
    {
        return Err(AppError::blocked("team state invariant 위반"));
    }
    let expected_status = match record.stage {
        TeamStage::Complete => "completed",
        TeamStage::Failed => "failed",
        TeamStage::Cancelled => "cancelled",
        _ => "active",
    };
    if record.status != expected_status {
        return Err(AppError::blocked("team state status/stage binding 불일치"));
    }
    if record.stage == TeamStage::Plan {
        if record.admitted_lanes != 0 || record.execution_mode != "pending" {
            return Err(AppError::blocked("team plan admission binding 불일치"));
        }
    } else if !record.stage.is_terminal()
        && (record.admitted_lanes == 0
            || !matches!(record.execution_mode.as_str(), "parallel" | "sequential"))
    {
        return Err(AppError::blocked(
            "team active stage admission binding 불일치",
        ));
    }
    if installed
        && (!is_sha256(&record.artifact_hash)
            || integrity::sha256_text(&render_payload(record)) != record.artifact_hash)
    {
        return Err(AppError::blocked("team state artifact hash 불일치"));
    }
    Ok(())
}

pub(crate) fn immutable_binding_changed(left: &TeamStateV1, right: &TeamStateV1) -> bool {
    left.team_id != right.team_id
        || left.manifest_hash != right.manifest_hash
        || left.project_id != right.project_id
        || left.session_id != right.session_id
        || left.parent_workflow_id != right.parent_workflow_id
        || left.parent_revision != right.parent_revision
        || left.parent_artifact_hash != right.parent_artifact_hash
        || left.requested_lanes != right.requested_lanes
        || left.member_count != right.member_count
        || left.created_at_ms != right.created_at_ms
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

pub(crate) fn validate_id(value: &str, label: &str, max_bytes: usize) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(AppError::blocked(format!("{label} 형식 오류: {value}")));
    }
    Ok(())
}

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
