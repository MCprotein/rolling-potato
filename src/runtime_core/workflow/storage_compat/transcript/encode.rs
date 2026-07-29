use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

use super::decode::parse_record;
use super::schema::{TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2};
use super::types::{ToolOutputArtifactBinding, TranscriptRecord, TranscriptSourcePointer};

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

pub(crate) fn canonical_install_bytes(
    record: &TranscriptRecord,
    existing: Option<&str>,
) -> Result<Option<String>, AppError> {
    let bytes = record.to_json();
    if bytes.len() > 128 * 1024 {
        return Err(AppError::blocked(
            "TranscriptRecord v2 canonical byte limit 초과",
        ));
    }
    if parse_record(&bytes)? != *record {
        return Err(AppError::blocked(
            "TranscriptRecord canonical codec round-trip 불일치",
        ));
    }
    if let Some(existing) = existing {
        if existing == bytes {
            return Ok(None);
        }
        return Err(AppError::blocked("TranscriptRecord immutable conflict"));
    }
    Ok(Some(bytes))
}
