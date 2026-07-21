use super::CompactionCheckpoint;
use crate::foundation::error::AppError;
use crate::foundation::integrity::sha256_text;
use crate::foundation::serialization as strict_json;

pub(crate) const COMPACTION_SCHEMA_VERSION: u64 = 1;
const MAX_ARTIFACT_BYTES: usize = 64 * 1024;
const ARTIFACT_KEYS: &[&str] = &[
    "schema_version",
    "artifact_id",
    "project_id",
    "session_id",
    "boundary_record_id",
    "previous_artifact_path",
    "previous_artifact_hash",
    "source_record_count",
    "source_records_dropped",
    "recent_record_ids",
    "checkpoint",
    "summary_model_id",
    "created_at_ms",
    "artifact_hash",
];
const CHECKPOINT_KEYS: &[&str] = &[
    "current_task",
    "constraints",
    "decisions",
    "files",
    "verification",
    "errors",
    "remaining_work",
    "artifact_refs",
    "unknowns",
    "rationale",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompactionArtifact {
    pub schema_version: u64,
    pub artifact_id: String,
    pub project_id: String,
    pub session_id: String,
    pub boundary_record_id: String,
    pub previous_artifact_path: String,
    pub previous_artifact_hash: String,
    pub source_record_count: u64,
    pub source_records_dropped: u64,
    pub recent_record_ids: Vec<String>,
    pub checkpoint: CompactionCheckpoint,
    pub summary_model_id: String,
    pub created_at_ms: u128,
    pub artifact_hash: String,
}

pub(crate) fn render_artifact_payload(artifact: &CompactionArtifact) -> String {
    format!(
        "{{\"schema_version\":{COMPACTION_SCHEMA_VERSION},\"artifact_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"boundary_record_id\":\"{}\",\"previous_artifact_path\":\"{}\",\"previous_artifact_hash\":\"{}\",\"source_record_count\":{},\"source_records_dropped\":{},\"recent_record_ids\":{},\"checkpoint\":{},\"summary_model_id\":\"{}\",\"created_at_ms\":{}}}",
        escape(&artifact.artifact_id),
        escape(&artifact.project_id),
        escape(&artifact.session_id),
        escape(&artifact.boundary_record_id),
        escape(&artifact.previous_artifact_path),
        escape(&artifact.previous_artifact_hash),
        artifact.source_record_count,
        artifact.source_records_dropped,
        render_string_array(&artifact.recent_record_ids),
        render_checkpoint(&artifact.checkpoint),
        escape(&artifact.summary_model_id),
        artifact.created_at_ms,
    )
}

pub(crate) fn render_artifact(artifact: &CompactionArtifact) -> String {
    let payload = render_artifact_payload(artifact);
    format!(
        "{},\"artifact_hash\":\"{}\"}}",
        payload.trim_end_matches('}'),
        artifact.artifact_hash
    )
}

pub(crate) fn parse_artifact(body: &str, context: &str) -> Result<CompactionArtifact, AppError> {
    if body.len() > MAX_ARTIFACT_BYTES {
        return Err(AppError::blocked(format!(
            "{context}: compaction artifact byte 상한 초과"
        )));
    }
    let object = strict_json::parse_canonical_object(body, ARTIFACT_KEYS, context)?;
    let checkpoint = parse_checkpoint(&object, context)?;
    let artifact = CompactionArtifact {
        schema_version: strict_json::canonical_u64(&object, "schema_version", context)?,
        artifact_id: canonical_string(&object, "artifact_id", context)?,
        project_id: canonical_string(&object, "project_id", context)?,
        session_id: canonical_string(&object, "session_id", context)?,
        boundary_record_id: canonical_string(&object, "boundary_record_id", context)?,
        previous_artifact_path: canonical_string(&object, "previous_artifact_path", context)?,
        previous_artifact_hash: canonical_string(&object, "previous_artifact_hash", context)?,
        source_record_count: strict_json::canonical_u64(&object, "source_record_count", context)?,
        source_records_dropped: strict_json::canonical_u64(
            &object,
            "source_records_dropped",
            context,
        )?,
        recent_record_ids: canonical_string_array(&object, "recent_record_ids", context)?,
        checkpoint,
        summary_model_id: canonical_string(&object, "summary_model_id", context)?,
        created_at_ms: strict_json::canonical_u128(&object, "created_at_ms", context)?,
        artifact_hash: canonical_string(&object, "artifact_hash", context)?,
    };
    validate_artifact(&artifact, context)?;
    if render_artifact(&artifact) != body {
        return Err(AppError::blocked(format!(
            "{context}: compaction artifact canonical re-render 불일치"
        )));
    }
    Ok(artifact)
}

fn validate_artifact(artifact: &CompactionArtifact, context: &str) -> Result<(), AppError> {
    let expected_prefix = format!(
        "state/compactions/{}/{}/",
        artifact.project_id, artifact.session_id
    );
    let previous_pair_valid = artifact.previous_artifact_path == "none"
        && artifact.previous_artifact_hash == "none"
        || artifact
            .previous_artifact_path
            .starts_with(&expected_prefix)
            && artifact.previous_artifact_path.ends_with(".json")
            && valid_hash(&artifact.previous_artifact_hash);
    let mut normalized = artifact.checkpoint.clone();
    normalized.normalize();
    if artifact.schema_version != COMPACTION_SCHEMA_VERSION
        || !valid_id(&artifact.artifact_id)
        || !artifact.artifact_id.starts_with("compaction-")
        || !valid_id(&artifact.project_id)
        || !valid_id(&artifact.session_id)
        || !valid_id(&artifact.boundary_record_id)
        || !previous_pair_valid
        || artifact.source_record_count == 0
        || artifact.source_records_dropped > artifact.source_record_count
        || artifact.recent_record_ids.len() > 4
        || artifact.recent_record_ids.iter().any(|id| !valid_id(id))
        || artifact.summary_model_id.trim().is_empty()
        || artifact.summary_model_id.len() > 256
        || artifact.checkpoint != normalized
        || artifact.checkpoint.current_task.is_empty()
        || !valid_hash(&artifact.artifact_hash)
        || artifact.artifact_hash != sha256_text(&render_artifact_payload(artifact))
    {
        return Err(AppError::blocked(format!(
            "{context}: compaction artifact binding/hash 불일치"
        )));
    }
    Ok(())
}

fn render_checkpoint(checkpoint: &CompactionCheckpoint) -> String {
    format!(
        "{{\"current_task\":\"{}\",\"constraints\":{},\"decisions\":{},\"files\":{},\"verification\":{},\"errors\":{},\"remaining_work\":{},\"artifact_refs\":{},\"unknowns\":{},\"rationale\":\"{}\"}}",
        escape(&checkpoint.current_task),
        render_string_array(&checkpoint.constraints),
        render_string_array(&checkpoint.decisions),
        render_string_array(&checkpoint.files),
        render_string_array(&checkpoint.verification),
        render_string_array(&checkpoint.errors),
        render_string_array(&checkpoint.remaining_work),
        render_string_array(&checkpoint.artifact_refs),
        render_string_array(&checkpoint.unknowns),
        escape(&checkpoint.rationale),
    )
}

fn parse_checkpoint(
    artifact: &strict_json::CanonicalObject,
    context: &str,
) -> Result<CompactionCheckpoint, AppError> {
    let Some(strict_json::CanonicalValue::Object(checkpoint)) = artifact.get("checkpoint") else {
        return Err(AppError::blocked(format!(
            "{context}: checkpoint object 누락"
        )));
    };
    let actual = checkpoint
        .entries
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    if actual != CHECKPOINT_KEYS {
        return Err(AppError::blocked(format!(
            "{context}: checkpoint key order 불일치"
        )));
    }
    Ok(CompactionCheckpoint {
        current_task: canonical_string(checkpoint, "current_task", context)?,
        constraints: canonical_string_array(checkpoint, "constraints", context)?,
        decisions: canonical_string_array(checkpoint, "decisions", context)?,
        files: canonical_string_array(checkpoint, "files", context)?,
        verification: canonical_string_array(checkpoint, "verification", context)?,
        errors: canonical_string_array(checkpoint, "errors", context)?,
        remaining_work: canonical_string_array(checkpoint, "remaining_work", context)?,
        artifact_refs: canonical_string_array(checkpoint, "artifact_refs", context)?,
        unknowns: canonical_string_array(checkpoint, "unknowns", context)?,
        rationale: canonical_string(checkpoint, "rationale", context)?,
    })
}

fn canonical_string(
    object: &strict_json::CanonicalObject,
    key: &str,
    context: &str,
) -> Result<String, AppError> {
    match object.get(key) {
        Some(strict_json::CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "{context}: missing/wrong string: {key}"
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
            "{context}: missing/wrong array: {key}"
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

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 192
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact() -> CompactionArtifact {
        let mut artifact = CompactionArtifact {
            schema_version: COMPACTION_SCHEMA_VERSION,
            artifact_id: "compaction-example".to_string(),
            project_id: "project-example".to_string(),
            session_id: "session-example".to_string(),
            boundary_record_id: "transcript-boundary".to_string(),
            previous_artifact_path: "none".to_string(),
            previous_artifact_hash: "none".to_string(),
            source_record_count: 12,
            source_records_dropped: 1,
            recent_record_ids: vec!["transcript-recent".to_string()],
            checkpoint: CompactionCheckpoint {
                current_task: "context 압축 구현".to_string(),
                constraints: vec!["원본 transcript 유지".to_string()],
                remaining_work: vec!["TUI 연결".to_string()],
                ..CompactionCheckpoint::default()
            },
            summary_model_id: "deterministic-fallback".to_string(),
            created_at_ms: 42,
            artifact_hash: String::new(),
        };
        artifact.artifact_hash = sha256_text(&render_artifact_payload(&artifact));
        artifact
    }

    #[test]
    fn artifact_round_trips_with_checkpoint_and_hash_binding() {
        let expected = artifact();
        let body = render_artifact(&expected);
        assert_eq!(parse_artifact(&body, "test artifact").unwrap(), expected);
    }

    #[test]
    fn artifact_rejects_tampering_and_cross_project_previous_pointer() {
        let expected = artifact();
        let body = render_artifact(&expected).replacen("TUI 연결", "release 연결", 1);
        assert!(parse_artifact(&body, "tampered artifact").is_err());

        let mut wrong_pointer = artifact();
        wrong_pointer.previous_artifact_path =
            "state/compactions/other/session-example/old.json".to_string();
        wrong_pointer.previous_artifact_hash = "a".repeat(64);
        wrong_pointer.artifact_hash = sha256_text(&render_artifact_payload(&wrong_pointer));
        assert!(parse_artifact(&render_artifact(&wrong_pointer), "wrong pointer").is_err());
    }
}
