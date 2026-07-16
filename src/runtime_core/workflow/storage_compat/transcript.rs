//! Canonical transcript DTOs, codecs, and durable record ownership.

use crate::foundation::serialization as strict_json;

const TRANSCRIPT_SCHEMA_V1: u64 = 1;
const TRANSCRIPT_SCHEMA_V2: u64 = 2;

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
