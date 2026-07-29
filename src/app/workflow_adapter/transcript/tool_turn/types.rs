use std::path::PathBuf;

use crate::app::workflow_adapter::ledger;
use crate::runtime_core::workflow::storage_compat::transcript::{
    ToolOutputArtifactBinding, TranscriptRecord,
};

use super::super::storage::tool_output_artifact_relative_path;

pub(in super::super) const MAX_SANITIZED_STREAM_BYTES: usize = 64 * 1024;
pub(in super::super) const MAX_TOOL_ARTIFACT_BYTES: usize = 256 * 1024;
pub(in super::super) const UNAVAILABLE_STREAM: &str = "<unavailable>";
pub(in super::super) const TOOL_ARTIFACT_KEYS: &[&str] = &[
    "schema_version",
    "artifact_id",
    "project_id",
    "session_id",
    "workflow_id",
    "tool_id",
    "created_at_ms",
    "redaction_policy",
    "redaction_version",
    "stdout",
    "stderr",
    "stdout_original_bytes",
    "stderr_original_bytes",
    "stdout_retained_chars",
    "stderr_retained_chars",
    "stdout_truncated",
    "stderr_truncated",
    "stdout_redacted",
    "stderr_redacted",
    "content_hash",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in super::super) struct SanitizedToolOutputArtifact {
    pub(in super::super) artifact_id: String,
    pub(in super::super) project_id: String,
    pub(in super::super) session_id: String,
    pub(in super::super) workflow_id: String,
    pub(in super::super) tool_id: String,
    pub(in super::super) created_at_ms: u128,
    pub(in super::super) stdout: String,
    pub(in super::super) stderr: String,
    pub(in super::super) stdout_original_bytes: u64,
    pub(in super::super) stderr_original_bytes: u64,
    pub(in super::super) stdout_retained_chars: u64,
    pub(in super::super) stderr_retained_chars: u64,
    pub(in super::super) stdout_truncated: bool,
    pub(in super::super) stderr_truncated: bool,
    pub(in super::super) stdout_redacted: bool,
    pub(in super::super) stderr_redacted: bool,
    pub(in super::super) content_hash: String,
}

impl SanitizedToolOutputArtifact {
    pub(in super::super) fn payload(&self) -> String {
        format!(
            "{{\"schema_version\":1,\"artifact_id\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"workflow_id\":\"{}\",\"tool_id\":\"{}\",\"created_at_ms\":{},\"redaction_policy\":\"credential-and-control-redaction\",\"redaction_version\":1,\"stdout\":\"{}\",\"stderr\":\"{}\",\"stdout_original_bytes\":{},\"stderr_original_bytes\":{},\"stdout_retained_chars\":{},\"stderr_retained_chars\":{},\"stdout_truncated\":{},\"stderr_truncated\":{},\"stdout_redacted\":{},\"stderr_redacted\":{}}}",
            ledger::json_string(&self.artifact_id),
            ledger::json_string(&self.project_id),
            ledger::json_string(&self.session_id),
            ledger::json_string(&self.workflow_id),
            ledger::json_string(&self.tool_id),
            self.created_at_ms,
            ledger::json_string(&self.stdout),
            ledger::json_string(&self.stderr),
            self.stdout_original_bytes,
            self.stderr_original_bytes,
            self.stdout_retained_chars,
            self.stderr_retained_chars,
            self.stdout_truncated,
            self.stderr_truncated,
            self.stdout_redacted,
            self.stderr_redacted,
        )
    }

    pub(in super::super) fn to_json(&self) -> String {
        format!(
            "{},\"content_hash\":\"{}\"}}",
            self.payload().trim_end_matches('}'),
            self.content_hash
        )
    }

    pub(in super::super) fn binding(&self) -> ToolOutputArtifactBinding {
        ToolOutputArtifactBinding {
            id: self.artifact_id.clone(),
            path: tool_output_artifact_relative_path(
                &self.project_id,
                &self.session_id,
                &self.workflow_id,
                &self.artifact_id,
            ),
            hash: self.content_hash.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedTranscriptTurn {
    pub tool_artifact_id: String,
    pub tool_path: PathBuf,
    pub tool_stored_path: String,
    pub tool_bytes: String,
    pub transcript_path: PathBuf,
    pub transcript_stored_path: String,
    pub transcript_bytes: String,
    pub record: TranscriptRecord,
    pub event: crate::app::workflow_adapter::ledger::LedgerEvent,
}
