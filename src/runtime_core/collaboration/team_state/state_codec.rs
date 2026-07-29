use super::{
    is_sha256, validate_id, TeamStage, TeamStateV1, MAX_STATE_REVISIONS, MAX_TEAM_ID_BYTES,
    MAX_TEAM_LANES, TEAM_SCHEMA_VERSION,
};
use crate::foundation::error::AppError;
use crate::foundation::integrity;
use crate::foundation::serialization as strict_json;

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
