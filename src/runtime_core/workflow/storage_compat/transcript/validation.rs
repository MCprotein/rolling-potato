use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::foundation::error::AppError;

use super::schema::{TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2};
use super::types::{TranscriptRecord, TranscriptSourcePointer};

pub(crate) fn validate_kind(kind: &str) -> Result<(), AppError> {
    if matches!(kind, "user" | "model" | "tool" | "evidence") {
        Ok(())
    } else {
        Err(AppError::blocked(format!("transcript kind 불일치: {kind}")))
    }
}

pub(crate) fn validate_id(label: &str, value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 160
        || matches!(value, "." | "..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(AppError::blocked(format!("transcript {label} 형식 불일치")));
    }
    Ok(())
}

pub(crate) fn validate_source_pointer(pointer: &TranscriptSourcePointer) -> Result<(), AppError> {
    if pointer.stable_ref.is_empty()
        || pointer.stable_ref.len() > 4_096
        || pointer.stable_ref.contains(['\r', '\n'])
        || pointer.path.is_empty()
        || pointer.path.len() > 4_096
        || pointer.path.contains(['\r', '\n'])
        || Path::new(&pointer.path).components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || pointer.source_hash.len() != 64
        || !pointer
            .source_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::blocked(
            "transcript source pointer boundary 불일치",
        ));
    }
    Ok(())
}

pub(crate) fn validate_sha256(label: &str, value: &str) -> Result<(), AppError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(AppError::blocked(format!("{label} 형식 불일치")))
    }
}

pub(crate) fn validate_tool_binding_shape(record: &TranscriptRecord) -> Result<(), AppError> {
    match (
        record.schema_version,
        record.kind.as_str(),
        &record.tool_output_artifact,
    ) {
        (TRANSCRIPT_SCHEMA_V1, _, None) => return Ok(()),
        (TRANSCRIPT_SCHEMA_V1, _, Some(_)) => {
            return Err(AppError::blocked(
                "TranscriptRecord v1 tool binding은 허용되지 않습니다.",
            ));
        }
        (TRANSCRIPT_SCHEMA_V2, "tool", Some(_)) => {}
        (TRANSCRIPT_SCHEMA_V2, "tool", None) => {
            return Err(AppError::blocked("TranscriptRecord v2 tool binding 누락"));
        }
        (TRANSCRIPT_SCHEMA_V2, _, None) => return Ok(()),
        (TRANSCRIPT_SCHEMA_V2, _, Some(_)) => {
            return Err(AppError::blocked(
                "TranscriptRecord v2 non-tool binding은 null이어야 합니다.",
            ));
        }
        _ => return Err(AppError::blocked("transcript schema version 불일치")),
    }

    let binding = record
        .tool_output_artifact
        .as_ref()
        .expect("tool binding checked above");
    validate_id("tool artifact id", &binding.id)?;
    validate_sha256("tool artifact hash", &binding.hash)?;
    let expected_path = tool_output_artifact_relative_path(
        &record.project_id,
        &record.session_id,
        &record.workflow_id,
        &binding.id,
    );
    if binding.path != expected_path {
        return Err(AppError::blocked(
            "TranscriptRecord v2 tool artifact path binding 불일치",
        ));
    }
    Ok(())
}

pub(crate) fn tool_output_artifact_relative_path(
    project_id: &str,
    session_id: &str,
    workflow_id: &str,
    artifact_id: &str,
) -> String {
    format!("state/tool-output/{project_id}/{session_id}/{workflow_id}/{artifact_id}.json")
}

pub(super) fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
