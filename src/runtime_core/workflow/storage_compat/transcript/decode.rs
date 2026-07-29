use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

use super::schema::{
    MAX_TRANSCRIPT_CONTENT_BYTES, TOOL_BINDING_KEYS, TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2,
    TRANSCRIPT_V1_KEYS, TRANSCRIPT_V2_KEYS,
};
use super::types::{ToolOutputArtifactBinding, TranscriptRecord, TranscriptSourcePointer};
use super::validation::{
    sha256_text, validate_id, validate_kind, validate_source_pointer, validate_tool_binding_shape,
};

pub(crate) fn parse_record(body: &str) -> Result<TranscriptRecord, AppError> {
    let version_probe =
        strict_json::parse_object(body, TRANSCRIPT_V2_KEYS, "transcript artifact version")?;
    let schema_version = strict_json::number(
        &version_probe,
        "schema_version",
        "transcript artifact version",
    )?;
    let mut record = match schema_version {
        TRANSCRIPT_SCHEMA_V1 => parse_v1(body)?,
        TRANSCRIPT_SCHEMA_V2 => parse_v2(body)?,
        _ => return Err(AppError::blocked("transcript schema version 불일치")),
    };
    validate_kind(&record.kind)?;
    validate_id("project id", &record.project_id)?;
    validate_id("record id", &record.record_id)?;
    validate_id("workflow id", &record.workflow_id)?;
    validate_id("session id", &record.session_id)?;
    validate_id("causal id", &record.causal_id)?;
    if record.content.trim().is_empty() || record.content.len() > MAX_TRANSCRIPT_CONTENT_BYTES {
        return Err(AppError::blocked(format!(
            "transcript content boundary 불일치\n- record id: {}",
            record.record_id
        )));
    }
    for pointer in &record.source_pointers {
        validate_source_pointer(pointer)?;
    }
    validate_tool_binding_shape(&record)?;
    if record.content_hash != sha256_text(&record.content) {
        return Err(AppError::blocked(format!(
            "transcript content hash 불일치\n- record id: {}",
            record.record_id
        )));
    }
    let expected_artifact_hash = sha256_text(&record.artifact_payload());
    if record.artifact_hash != expected_artifact_hash {
        return Err(AppError::blocked(format!(
            "transcript artifact hash 불일치\n- record id: {}",
            record.record_id
        )));
    }
    record.artifact_hash = expected_artifact_hash;
    Ok(record)
}

fn parse_v1(body: &str) -> Result<TranscriptRecord, AppError> {
    let object = strict_json::parse_object(body, TRANSCRIPT_V1_KEYS, "transcript v1")?;
    if strict_json::number(&object, "schema_version", "transcript v1")? != TRANSCRIPT_SCHEMA_V1 {
        return Err(AppError::blocked("transcript v1 schema 불일치"));
    }
    Ok(TranscriptRecord {
        schema_version: TRANSCRIPT_SCHEMA_V1,
        record_id: strict_json::string(&object, "record_id", "transcript v1")?,
        project_id: strict_json::string(&object, "project_id", "transcript v1")?,
        session_id: strict_json::string(&object, "session_id", "transcript v1")?,
        workflow_id: strict_json::string(&object, "workflow_id", "transcript v1")?,
        kind: strict_json::string(&object, "kind", "transcript v1")?,
        causal_id: strict_json::string(&object, "causal_id", "transcript v1")?,
        content: strict_json::string(&object, "content", "transcript v1")?,
        content_hash: strict_json::string(&object, "content_hash", "transcript v1")?,
        source_pointers: parse_source_pointers(object.get("source_pointers"))?,
        recorded_at_ms: strict_json::number_u128(&object, "recorded_at_ms", "transcript v1")?,
        tool_output_artifact: None,
        artifact_hash: strict_json::string(&object, "artifact_hash", "transcript v1")?,
    })
}

fn parse_v2(body: &str) -> Result<TranscriptRecord, AppError> {
    use strict_json::CanonicalValue;

    let object =
        strict_json::parse_canonical_object(body, TRANSCRIPT_V2_KEYS, "TranscriptRecord v2")?;
    if strict_json::canonical_u64(&object, "schema_version", "TranscriptRecord v2")?
        != TRANSCRIPT_SCHEMA_V2
    {
        return Err(AppError::blocked("TranscriptRecord v2 schema 불일치"));
    }
    let string = |key: &str| match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "TranscriptRecord v2 field type 불일치: {key}"
        ))),
    };
    Ok(TranscriptRecord {
        schema_version: TRANSCRIPT_SCHEMA_V2,
        record_id: string("record_id")?,
        project_id: string("project_id")?,
        session_id: string("session_id")?,
        workflow_id: string("workflow_id")?,
        kind: string("kind")?,
        causal_id: string("causal_id")?,
        content: string("content")?,
        content_hash: string("content_hash")?,
        source_pointers: parse_canonical_source_pointers(object.get("source_pointers"))?,
        recorded_at_ms: strict_json::canonical_u128(
            &object,
            "recorded_at_ms",
            "TranscriptRecord v2",
        )?,
        tool_output_artifact: parse_tool_binding(object.get("tool_output_artifact"))?,
        artifact_hash: string("artifact_hash")?,
    })
}

fn parse_source_pointers(
    value: Option<&strict_json::Value>,
) -> Result<Vec<TranscriptSourcePointer>, AppError> {
    let Some(strict_json::Value::Array(values)) = value else {
        return Err(AppError::blocked("transcript source_pointers type 불일치"));
    };
    let mut pointers = Vec::new();
    for value in values {
        let strict_json::Value::Object(object) = value else {
            return Err(AppError::blocked("transcript source pointer type 불일치"));
        };
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "stable_ref" | "path" | "source_hash"))
        {
            return Err(AppError::blocked("transcript source pointer key 불일치"));
        }
        pointers.push(TranscriptSourcePointer {
            stable_ref: strict_json::string(object, "stable_ref", "transcript pointer")?,
            path: strict_json::string(object, "path", "transcript pointer")?,
            source_hash: strict_json::string(object, "source_hash", "transcript pointer")?,
        });
    }
    Ok(pointers)
}

fn parse_canonical_source_pointers(
    value: Option<&strict_json::CanonicalValue>,
) -> Result<Vec<TranscriptSourcePointer>, AppError> {
    use strict_json::CanonicalValue;

    let Some(CanonicalValue::Array(values)) = value else {
        return Err(AppError::blocked(
            "TranscriptRecord v2 source_pointers type 불일치",
        ));
    };
    values
        .iter()
        .map(|value| {
            let CanonicalValue::Object(object) = value else {
                return Err(AppError::blocked(
                    "TranscriptRecord v2 source pointer type 불일치",
                ));
            };
            let keys = object
                .entries
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>();
            if keys != ["stable_ref", "path", "source_hash"] {
                return Err(AppError::blocked(
                    "TranscriptRecord v2 source pointer key/order 불일치",
                ));
            }
            let string = |key: &str| match object.get(key) {
                Some(CanonicalValue::String(value)) => Ok(value.clone()),
                _ => Err(AppError::blocked(format!(
                    "TranscriptRecord v2 source pointer field 불일치: {key}"
                ))),
            };
            Ok(TranscriptSourcePointer {
                stable_ref: string("stable_ref")?,
                path: string("path")?,
                source_hash: string("source_hash")?,
            })
        })
        .collect()
}

fn parse_tool_binding(
    value: Option<&strict_json::CanonicalValue>,
) -> Result<Option<ToolOutputArtifactBinding>, AppError> {
    use strict_json::CanonicalValue;

    match value {
        Some(CanonicalValue::Null) => Ok(None),
        Some(CanonicalValue::Object(object)) => {
            let keys = object
                .entries
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>();
            if keys != TOOL_BINDING_KEYS {
                return Err(AppError::blocked(
                    "TranscriptRecord v2 tool binding key/order 불일치",
                ));
            }
            let string = |key: &str| match object.get(key) {
                Some(CanonicalValue::String(value)) => Ok(value.clone()),
                _ => Err(AppError::blocked(format!(
                    "TranscriptRecord v2 tool binding field 불일치: {key}"
                ))),
            };
            Ok(Some(ToolOutputArtifactBinding {
                id: string("id")?,
                path: string("path")?,
                hash: string("hash")?,
            }))
        }
        _ => Err(AppError::blocked(
            "TranscriptRecord v2 tool_output_artifact type 불일치",
        )),
    }
}
