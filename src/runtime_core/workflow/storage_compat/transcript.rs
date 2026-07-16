//! Canonical transcript DTOs, codecs, and durable record ownership.

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
