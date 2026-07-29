pub(crate) const TRANSCRIPT_SCHEMA_V1: u64 = 1;
pub(crate) const TRANSCRIPT_SCHEMA_V2: u64 = 2;
pub(crate) const MAX_TRANSCRIPT_CONTENT_BYTES: usize = 64 * 1024;

pub(super) const TRANSCRIPT_V1_KEYS: &[&str] = &[
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

pub(super) const TOOL_BINDING_KEYS: &[&str] = &["id", "path", "hash"];
