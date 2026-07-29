use crate::adapters::filesystem::layout as paths;
use crate::app::context_adapter::SourcePointer;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::transcript::{
    TranscriptRecord, TranscriptSourcePointer, MAX_TRANSCRIPT_CONTENT_BYTES, TRANSCRIPT_SCHEMA_V2,
};

use super::super::storage::{
    now_ms, validate_id, validate_source_pointer, validate_tool_binding_shape_for_record,
};
use super::super::transcript_ledger_event;
use super::streams::sanitize_tool_stream;
use super::types::{PreparedTranscriptTurn, SanitizedToolOutputArtifact, MAX_TOOL_ARTIFACT_BYTES};

pub(crate) fn prepare_no_stream_tool_turn(
    workflow: &state::WorkflowRecord,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
) -> Result<PreparedTranscriptTurn, AppError> {
    validate_id("project id", &workflow.project_id)?;
    validate_id("workflow id", &workflow.workflow_id)?;
    validate_id("session id", &workflow.session_id)?;
    validate_id("causal id", causal_id)?;
    if content.trim().is_empty() || content.len() > MAX_TRANSCRIPT_CONTENT_BYTES {
        return Err(AppError::blocked(
            "prepared transcript content boundary 불일치",
        ));
    }
    let created_at_ms = now_ms();
    let tool_artifact_id = format!(
        "tool-output-{}",
        state::sha256_text(
            &[
                "rpotato.tool-output-artifact-id/v1",
                &workflow.project_id,
                &workflow.session_id,
                &workflow.workflow_id,
                causal_id,
            ]
            .join("\0")
        )
    );
    let stdout = sanitize_tool_stream(None)?;
    let stderr = sanitize_tool_stream(None)?;
    let mut artifact = SanitizedToolOutputArtifact {
        artifact_id: tool_artifact_id.clone(),
        project_id: workflow.project_id.clone(),
        session_id: workflow.session_id.clone(),
        workflow_id: workflow.workflow_id.clone(),
        tool_id: causal_id.to_string(),
        created_at_ms,
        stdout: stdout.text,
        stderr: stderr.text,
        stdout_original_bytes: stdout.original_bytes,
        stderr_original_bytes: stderr.original_bytes,
        stdout_retained_chars: stdout.retained_chars,
        stderr_retained_chars: stderr.retained_chars,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
        stdout_redacted: stdout.redacted,
        stderr_redacted: stderr.redacted,
        content_hash: String::new(),
    };
    artifact.content_hash = state::sha256_text(&artifact.payload());
    let tool_bytes = artifact.to_json();
    if tool_bytes.len() > MAX_TOOL_ARTIFACT_BYTES {
        return Err(AppError::blocked(
            "prepared SanitizedToolOutputArtifact byte limit 초과",
        ));
    }
    let binding = artifact.binding();
    let pointers = source_pointers
        .iter()
        .map(|pointer| {
            let pointer = TranscriptSourcePointer {
                stable_ref: pointer.stable_ref.clone(),
                path: pointer.path.clone(),
                source_hash: pointer.fingerprint.clone(),
            };
            validate_source_pointer(&pointer)?;
            Ok(pointer)
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let record_id = format!(
        "transcript-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\ntool\n{}",
            workflow.project_id, workflow.session_id, workflow.workflow_id, causal_id
        ))[..24]
    );
    let mut record = TranscriptRecord {
        schema_version: TRANSCRIPT_SCHEMA_V2,
        record_id: record_id.clone(),
        project_id: workflow.project_id.clone(),
        session_id: workflow.session_id.clone(),
        workflow_id: workflow.workflow_id.clone(),
        kind: "tool".to_string(),
        causal_id: causal_id.to_string(),
        content: content.to_string(),
        content_hash: state::sha256_text(content),
        source_pointers: pointers,
        recorded_at_ms: created_at_ms,
        tool_output_artifact: Some(binding.clone()),
        artifact_hash: String::new(),
    };
    validate_tool_binding_shape_for_record(&record)?;
    record.artifact_hash = state::sha256_text(&record.artifact_payload());
    let transcript_bytes = record.to_json();
    if transcript_bytes.len() > 128 * 1024 {
        return Err(AppError::blocked(
            "prepared TranscriptRecord v2 byte limit 초과",
        ));
    }
    let event = transcript_ledger_event(&record)?;
    Ok(PreparedTranscriptTurn {
        tool_path: paths::tool_output_file(
            &workflow.project_id,
            &workflow.session_id,
            &workflow.workflow_id,
            &tool_artifact_id,
        ),
        tool_stored_path: binding.path,
        tool_artifact_id,
        tool_bytes,
        transcript_path: paths::transcript_file(
            &workflow.project_id,
            &workflow.session_id,
            &record_id,
        ),
        transcript_stored_path: format!(
            "state/transcripts/{}/{}/{}.json",
            workflow.project_id, workflow.session_id, record_id
        ),
        transcript_bytes,
        record,
        event,
    })
}
