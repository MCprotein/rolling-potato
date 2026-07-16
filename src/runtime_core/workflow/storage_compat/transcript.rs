//! Canonical transcript DTOs, codecs, and durable record ownership.

use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

pub(crate) const TRANSCRIPT_SCHEMA_V1: u64 = 1;
pub(crate) const TRANSCRIPT_SCHEMA_V2: u64 = 2;
pub(crate) const MAX_TRANSCRIPT_CONTENT_BYTES: usize = 64 * 1024;
const TRANSCRIPT_V1_KEYS: &[&str] = &[
    "schema_version",
    "record_id",
    "project_id",
    "session_id",
    "workflow_id",
    "kind",
    "causal_id",
    "content",
    "content_hash",
    "source_pointers",
    "recorded_at_ms",
    "artifact_hash",
];
pub(crate) const TRANSCRIPT_V2_KEYS: &[&str] = &[
    "schema_version",
    "record_id",
    "project_id",
    "session_id",
    "workflow_id",
    "kind",
    "causal_id",
    "content",
    "content_hash",
    "source_pointers",
    "recorded_at_ms",
    "tool_output_artifact",
    "artifact_hash",
];
const TOOL_BINDING_KEYS: &[&str] = &["id", "path", "hash"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptSourcePointer {
    pub stable_ref: String,
    pub path: String,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutputArtifactBinding {
    pub id: String,
    pub path: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRecord {
    pub schema_version: u64,
    pub record_id: String,
    pub project_id: String,
    pub session_id: String,
    pub workflow_id: String,
    pub kind: String,
    pub causal_id: String,
    pub content: String,
    pub content_hash: String,
    pub source_pointers: Vec<TranscriptSourcePointer>,
    pub recorded_at_ms: u128,
    pub tool_output_artifact: Option<ToolOutputArtifactBinding>,
    pub artifact_hash: String,
}

fn render_source_pointers(pointers: &[TranscriptSourcePointer]) -> String {
    let rows = pointers
        .iter()
        .map(|pointer| {
            format!(
                "{{\"stable_ref\":\"{}\",\"path\":\"{}\",\"source_hash\":\"{}\"}}",
                strict_json::escape_string_content(&pointer.stable_ref),
                strict_json::escape_string_content(&pointer.path),
                strict_json::escape_string_content(&pointer.source_hash)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{rows}]")
}

fn render_tool_binding(binding: Option<&ToolOutputArtifactBinding>) -> String {
    binding.map_or_else(
        || "null".to_string(),
        |binding| {
            format!(
                "{{\"id\":\"{}\",\"path\":\"{}\",\"hash\":\"{}\"}}",
                strict_json::escape_string_content(&binding.id),
                strict_json::escape_string_content(&binding.path),
                binding.hash
            )
        },
    )
}

impl TranscriptRecord {
    pub fn source_pointers_json(&self) -> String {
        render_source_pointers(&self.source_pointers)
    }

    pub(crate) fn artifact_payload(&self) -> String {
        match self.schema_version {
            TRANSCRIPT_SCHEMA_V1 => format!(
                "{{\"schema_version\":1,\"record_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"kind\":\"{}\",\"causal_id\":\"{}\",\"content\":\"{}\",\"content_hash\":\"{}\",\"source_pointers\":{},\"recorded_at_ms\":{}}}",
                strict_json::escape_string_content(&self.record_id),
                strict_json::escape_string_content(&self.project_id),
                strict_json::escape_string_content(&self.session_id),
                strict_json::escape_string_content(&self.workflow_id),
                strict_json::escape_string_content(&self.kind),
                strict_json::escape_string_content(&self.causal_id),
                strict_json::escape_string_content(&self.content),
                self.content_hash,
                render_source_pointers(&self.source_pointers),
                self.recorded_at_ms
            ),
            TRANSCRIPT_SCHEMA_V2 => format!(
                "{{\"schema_version\":2,\"record_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"kind\":\"{}\",\"causal_id\":\"{}\",\"content\":\"{}\",\"content_hash\":\"{}\",\"source_pointers\":{},\"recorded_at_ms\":{},\"tool_output_artifact\":{}}}",
                strict_json::escape_string_content(&self.record_id),
                strict_json::escape_string_content(&self.project_id),
                strict_json::escape_string_content(&self.session_id),
                strict_json::escape_string_content(&self.workflow_id),
                strict_json::escape_string_content(&self.kind),
                strict_json::escape_string_content(&self.causal_id),
                strict_json::escape_string_content(&self.content),
                self.content_hash,
                render_source_pointers(&self.source_pointers),
                self.recorded_at_ms,
                render_tool_binding(self.tool_output_artifact.as_ref())
            ),
            _ => String::new(),
        }
    }

    pub(crate) fn to_json(&self) -> String {
        match self.schema_version {
            TRANSCRIPT_SCHEMA_V1 => format!(
                "{{\n  \"schema_version\": 1,\n  \"record_id\": \"{}\",\n  \"project_id\": \"{}\",\n  \"session_id\": \"{}\",\n  \"workflow_id\": \"{}\",\n  \"kind\": \"{}\",\n  \"causal_id\": \"{}\",\n  \"content\": \"{}\",\n  \"content_hash\": \"{}\",\n  \"source_pointers\": {},\n  \"recorded_at_ms\": {},\n  \"artifact_hash\": \"{}\"\n}}\n",
                strict_json::escape_string_content(&self.record_id),
                strict_json::escape_string_content(&self.project_id),
                strict_json::escape_string_content(&self.session_id),
                strict_json::escape_string_content(&self.workflow_id),
                strict_json::escape_string_content(&self.kind),
                strict_json::escape_string_content(&self.causal_id),
                strict_json::escape_string_content(&self.content),
                self.content_hash,
                render_source_pointers(&self.source_pointers),
                self.recorded_at_ms,
                self.artifact_hash
            ),
            TRANSCRIPT_SCHEMA_V2 => format!(
                "{{\"schema_version\":2,\"record_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"kind\":\"{}\",\"causal_id\":\"{}\",\"content\":\"{}\",\"content_hash\":\"{}\",\"source_pointers\":{},\"recorded_at_ms\":{},\"tool_output_artifact\":{},\"artifact_hash\":\"{}\"}}",
                strict_json::escape_string_content(&self.record_id),
                strict_json::escape_string_content(&self.project_id),
                strict_json::escape_string_content(&self.session_id),
                strict_json::escape_string_content(&self.workflow_id),
                strict_json::escape_string_content(&self.kind),
                strict_json::escape_string_content(&self.causal_id),
                strict_json::escape_string_content(&self.content),
                self.content_hash,
                render_source_pointers(&self.source_pointers),
                self.recorded_at_ms,
                render_tool_binding(self.tool_output_artifact.as_ref()),
                self.artifact_hash
            ),
            _ => String::new(),
        }
    }
}

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

fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
