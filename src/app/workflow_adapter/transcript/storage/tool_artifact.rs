use std::fs;
use std::io::Read;
use std::path::Path;

use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::workflow::storage_compat::transcript::{
    self as transcript_codec, TranscriptRecord,
};

use super::super::tool_turn::{
    SanitizedToolOutputArtifact, MAX_SANITIZED_STREAM_BYTES, MAX_TOOL_ARTIFACT_BYTES,
    TOOL_ARTIFACT_KEYS,
};
use super::path_resolution::validated_tool_output_path;

pub(in super::super) fn load_tool_output_artifact(
    path: &Path,
) -> Result<SanitizedToolOutputArtifact, AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        AppError::blocked(format!(
            "SanitizedToolOutputArtifact metadata 실패\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact regular-file boundary 불일치",
        ));
    }
    if metadata.len() > u64::try_from(MAX_TOOL_ARTIFACT_BYTES).unwrap_or(u64::MAX) {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact canonical byte limit 초과",
        ));
    }
    let mut file = fs::File::open(path).map_err(|err| {
        AppError::blocked(format!(
            "SanitizedToolOutputArtifact 읽기 실패\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref()
        .take(u64::try_from(MAX_TOOL_ARTIFACT_BYTES + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut bytes)
        .map_err(|err| {
            AppError::blocked(format!(
                "SanitizedToolOutputArtifact bounded 읽기 실패: {err}"
            ))
        })?;
    if bytes.len() > MAX_TOOL_ARTIFACT_BYTES {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact canonical byte limit 초과",
        ));
    }
    let body = String::from_utf8(bytes)
        .map_err(|_| AppError::blocked("SanitizedToolOutputArtifact UTF-8 불일치"))?;
    parse_tool_output_artifact_body(&body)
}

pub(in super::super) fn parse_tool_output_artifact_body(
    body: &str,
) -> Result<SanitizedToolOutputArtifact, AppError> {
    use strict_json::CanonicalValue;

    if body.len() > MAX_TOOL_ARTIFACT_BYTES {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact canonical byte limit 초과",
        ));
    }
    let object = strict_json::parse_canonical_object(
        body,
        TOOL_ARTIFACT_KEYS,
        "SanitizedToolOutputArtifact",
    )?;
    if strict_json::canonical_u64(&object, "schema_version", "SanitizedToolOutputArtifact")? != 1
        || string_from_canonical(&object, "redaction_policy")? != "credential-and-control-redaction"
        || strict_json::canonical_u64(&object, "redaction_version", "SanitizedToolOutputArtifact")?
            != 1
    {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact schema/policy 불일치",
        ));
    }
    let boolean = |key: &str| match object.get(key) {
        Some(CanonicalValue::Bool(value)) => Ok(*value),
        _ => Err(AppError::blocked(format!(
            "SanitizedToolOutputArtifact boolean field 불일치: {key}"
        ))),
    };
    let artifact = SanitizedToolOutputArtifact {
        artifact_id: string_from_canonical(&object, "artifact_id")?,
        project_id: string_from_canonical(&object, "project_id")?,
        session_id: string_from_canonical(&object, "session_id")?,
        workflow_id: string_from_canonical(&object, "workflow_id")?,
        tool_id: string_from_canonical(&object, "tool_id")?,
        created_at_ms: strict_json::canonical_u128(
            &object,
            "created_at_ms",
            "SanitizedToolOutputArtifact",
        )?,
        stdout: string_from_canonical(&object, "stdout")?,
        stderr: string_from_canonical(&object, "stderr")?,
        stdout_original_bytes: strict_json::canonical_u64(
            &object,
            "stdout_original_bytes",
            "SanitizedToolOutputArtifact",
        )?,
        stderr_original_bytes: strict_json::canonical_u64(
            &object,
            "stderr_original_bytes",
            "SanitizedToolOutputArtifact",
        )?,
        stdout_retained_chars: strict_json::canonical_u64(
            &object,
            "stdout_retained_chars",
            "SanitizedToolOutputArtifact",
        )?,
        stderr_retained_chars: strict_json::canonical_u64(
            &object,
            "stderr_retained_chars",
            "SanitizedToolOutputArtifact",
        )?,
        stdout_truncated: boolean("stdout_truncated")?,
        stderr_truncated: boolean("stderr_truncated")?,
        stdout_redacted: boolean("stdout_redacted")?,
        stderr_redacted: boolean("stderr_redacted")?,
        content_hash: string_from_canonical(&object, "content_hash")?,
    };
    transcript_codec::validate_id("tool artifact id", &artifact.artifact_id)?;
    transcript_codec::validate_id("tool id", &artifact.tool_id)?;
    transcript_codec::validate_id("project id", &artifact.project_id)?;
    transcript_codec::validate_id("session id", &artifact.session_id)?;
    transcript_codec::validate_id("workflow id", &artifact.workflow_id)?;
    transcript_codec::validate_sha256("tool artifact content hash", &artifact.content_hash)?;
    if artifact.stdout.len() > MAX_SANITIZED_STREAM_BYTES
        || artifact.stderr.len() > MAX_SANITIZED_STREAM_BYTES
        || artifact.stdout_retained_chars
            != u64::try_from(artifact.stdout.chars().count())
                .map_err(|_| AppError::blocked("stdout retained count overflow"))?
        || artifact.stderr_retained_chars
            != u64::try_from(artifact.stderr.chars().count())
                .map_err(|_| AppError::blocked("stderr retained count overflow"))?
        || artifact.content_hash != state::sha256_text(&artifact.payload())
        || artifact.to_json() != body
    {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact byte/count/hash binding 불일치",
        ));
    }
    Ok(artifact)
}

fn string_from_canonical(
    object: &strict_json::CanonicalObject,
    key: &str,
) -> Result<String, AppError> {
    match object.get(key) {
        Some(strict_json::CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "canonical string field 불일치: {key}"
        ))),
    }
}

pub(in super::super) fn validate_tool_binding_for_record(
    record: &TranscriptRecord,
) -> Result<(), AppError> {
    validate_tool_binding_shape_for_record(record)?;
    let Some(binding) = record.tool_output_artifact.as_ref() else {
        return Ok(());
    };
    let path = validated_tool_output_path(
        &record.project_id,
        &record.session_id,
        &record.workflow_id,
        &binding.id,
        false,
    )?;
    let artifact = load_tool_output_artifact(&path)?;
    if artifact.artifact_id != binding.id
        || artifact.project_id != record.project_id
        || artifact.session_id != record.session_id
        || artifact.workflow_id != record.workflow_id
        || artifact.tool_id != record.causal_id
        || artifact.content_hash != binding.hash
    {
        return Err(AppError::blocked(
            "TranscriptRecord v2 tool artifact owner/hash binding 불일치",
        ));
    }
    Ok(())
}

pub(in super::super) fn validate_tool_binding_shape_for_record(
    record: &TranscriptRecord,
) -> Result<(), AppError> {
    transcript_codec::validate_tool_binding_shape(record)
}

pub(in super::super) fn validate_tool_artifact_owner(
    artifact: &SanitizedToolOutputArtifact,
    workflow: &state::WorkflowRecord,
    tool_id: &str,
    artifact_id: &str,
) -> Result<(), AppError> {
    if artifact.artifact_id != artifact_id
        || artifact.project_id != workflow.project_id
        || artifact.session_id != workflow.session_id
        || artifact.workflow_id != workflow.workflow_id
        || artifact.tool_id != tool_id
    {
        return Err(AppError::blocked(
            "SanitizedToolOutputArtifact deterministic owner 충돌",
        ));
    }
    Ok(())
}
