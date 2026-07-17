use super::{validate_record, SubagentRecordV1, SubagentRole, SubagentStatus};
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

const SUBAGENT_SCHEMA_VERSION: u64 = 1;
const RECORD_KEYS: &[&str] = &[
    "schema_version",
    "subagent_id",
    "revision",
    "previous_hash",
    "artifact_hash",
    "project_id",
    "session_id",
    "parent_workflow_id",
    "parent_revision",
    "parent_artifact_hash",
    "role",
    "task_hash",
    "declared_tools",
    "read_paths",
    "write_paths",
    "timeout_ms",
    "requested_max_tokens",
    "effective_max_tokens",
    "status",
    "backend_event_id",
    "result_artifact_id",
    "result_artifact_hash",
    "evidence_id",
    "evidence_hash",
    "failure_code",
    "created_at_ms",
    "started_at_ms",
    "finished_at_ms",
];

pub(crate) fn render_payload(record: &SubagentRecordV1) -> String {
    format!(
        "{{\"schema_version\":{SUBAGENT_SCHEMA_VERSION},\"subagent_id\":\"{}\",\"revision\":{},\"previous_hash\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"parent_revision\":{},\"parent_artifact_hash\":\"{}\",\"role\":\"{}\",\"task_hash\":\"{}\",\"declared_tools\":{},\"read_paths\":{},\"write_paths\":{},\"timeout_ms\":{},\"requested_max_tokens\":{},\"effective_max_tokens\":{},\"status\":\"{}\",\"backend_event_id\":\"{}\",\"result_artifact_id\":\"{}\",\"result_artifact_hash\":\"{}\",\"evidence_id\":\"{}\",\"evidence_hash\":\"{}\",\"failure_code\":\"{}\",\"created_at_ms\":{},\"started_at_ms\":{},\"finished_at_ms\":{}}}",
        escape(&record.subagent_id),
        record.revision,
        escape(&record.previous_hash),
        escape(&record.project_id),
        escape(&record.session_id),
        escape(&record.parent_workflow_id),
        record.parent_revision,
        escape(&record.parent_artifact_hash),
        escape(record.role.as_str()),
        escape(&record.task_hash),
        render_string_array(&record.declared_tools),
        render_string_array(&record.read_paths),
        render_string_array(&record.write_paths),
        record.timeout_ms,
        record.requested_max_tokens,
        record.effective_max_tokens,
        escape(record.status.as_str()),
        escape(&record.backend_event_id),
        escape(&record.result_artifact_id),
        escape(&record.result_artifact_hash),
        escape(&record.evidence_id),
        escape(&record.evidence_hash),
        escape(&record.failure_code),
        record.created_at_ms,
        record.started_at_ms,
        record.finished_at_ms,
    )
}

pub(crate) fn render_record(record: &SubagentRecordV1) -> String {
    let payload = render_payload(record);
    let marker = format!("\"previous_hash\":\"{}\",", escape(&record.previous_hash));
    let replacement = format!(
        "{marker}\"artifact_hash\":\"{}\",",
        escape(&record.artifact_hash)
    );
    payload.replacen(&marker, &replacement, 1)
}

pub(crate) fn parse_record(context: &str, body: &str) -> Result<SubagentRecordV1, AppError> {
    let object = strict_json::parse_canonical_object(body, RECORD_KEYS, context)?;
    let role_text = canonical_string(&object, "role", context)?;
    let status_text = canonical_string(&object, "status", context)?;
    let record = SubagentRecordV1 {
        subagent_id: canonical_string(&object, "subagent_id", context)?,
        revision: strict_json::canonical_u64(&object, "revision", context)?,
        previous_hash: canonical_string(&object, "previous_hash", context)?,
        artifact_hash: canonical_string(&object, "artifact_hash", context)?,
        project_id: canonical_string(&object, "project_id", context)?,
        session_id: canonical_string(&object, "session_id", context)?,
        parent_workflow_id: canonical_string(&object, "parent_workflow_id", context)?,
        parent_revision: strict_json::canonical_u64(&object, "parent_revision", context)?,
        parent_artifact_hash: canonical_string(&object, "parent_artifact_hash", context)?,
        role: SubagentRole::parse(&role_text)
            .ok_or_else(|| AppError::blocked(format!("{context}: role 오류")))?,
        task_hash: canonical_string(&object, "task_hash", context)?,
        declared_tools: canonical_string_array(&object, "declared_tools", context)?,
        read_paths: canonical_string_array(&object, "read_paths", context)?,
        write_paths: canonical_string_array(&object, "write_paths", context)?,
        timeout_ms: canonical_u32(&object, "timeout_ms", context)?,
        requested_max_tokens: canonical_u32(&object, "requested_max_tokens", context)?,
        effective_max_tokens: canonical_u32(&object, "effective_max_tokens", context)?,
        status: SubagentStatus::parse(&status_text)
            .ok_or_else(|| AppError::blocked(format!("{context}: status 오류")))?,
        backend_event_id: canonical_string(&object, "backend_event_id", context)?,
        result_artifact_id: canonical_string(&object, "result_artifact_id", context)?,
        result_artifact_hash: canonical_string(&object, "result_artifact_hash", context)?,
        evidence_id: canonical_string(&object, "evidence_id", context)?,
        evidence_hash: canonical_string(&object, "evidence_hash", context)?,
        failure_code: canonical_string(&object, "failure_code", context)?,
        created_at_ms: strict_json::canonical_u128(&object, "created_at_ms", context)?,
        started_at_ms: strict_json::canonical_u128(&object, "started_at_ms", context)?,
        finished_at_ms: strict_json::canonical_u128(&object, "finished_at_ms", context)?,
    };
    if strict_json::canonical_u64(&object, "schema_version", context)? != SUBAGENT_SCHEMA_VERSION
        || render_record(&record) != body
    {
        return Err(AppError::blocked(format!(
            "{context}: schema 또는 canonical re-render 불일치"
        )));
    }
    validate_record(&record, true)?;
    Ok(record)
}

fn canonical_string(
    object: &strict_json::CanonicalObject,
    key: &str,
    context: &str,
) -> Result<String, AppError> {
    match object.get(key) {
        Some(strict_json::CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "{context}: missing/wrong type: {key}"
        ))),
    }
}

fn canonical_string_array(
    object: &strict_json::CanonicalObject,
    key: &str,
    context: &str,
) -> Result<Vec<String>, AppError> {
    let Some(strict_json::CanonicalValue::Array(values)) = object.get(key) else {
        return Err(AppError::blocked(format!(
            "{context}: missing/wrong type: {key}"
        )));
    };
    values
        .iter()
        .map(|value| match value {
            strict_json::CanonicalValue::String(value) => Ok(value.clone()),
            _ => Err(AppError::blocked(format!(
                "{context}: array item type 오류: {key}"
            ))),
        })
        .collect()
}

fn canonical_u32(
    object: &strict_json::CanonicalObject,
    key: &str,
    context: &str,
) -> Result<u32, AppError> {
    u32::try_from(strict_json::canonical_u64(object, key, context)?)
        .map_err(|_| AppError::blocked(format!("{context}: out of range: {key}")))
}

fn render_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", escape(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn escape(value: &str) -> String {
    strict_json::escape_string_content(value)
}
