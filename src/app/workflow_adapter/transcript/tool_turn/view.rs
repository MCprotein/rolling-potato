use crate::foundation::error::AppError;
use crate::runtime_core::workflow::domain::transcript::ToolOutputView;
use crate::runtime_core::workflow::storage_compat::transcript::{
    TranscriptRecord, TRANSCRIPT_SCHEMA_V2,
};

use super::super::storage::{
    load_tool_output_artifact, validate_id, validate_tool_binding_for_record,
    validated_tool_output_path,
};
use super::types::SanitizedToolOutputArtifact;

pub(crate) fn tool_output_view_from_canonical_record(
    record: &TranscriptRecord,
    artifact_id: &str,
) -> Result<ToolOutputView, AppError> {
    validate_id("tool artifact id", artifact_id)?;
    validate_tool_binding_for_record(record)?;
    let binding = record.tool_output_artifact.as_ref().ok_or_else(|| {
        AppError::blocked("tool-output view에 대응하는 TranscriptRecord v2 binding이 없습니다.")
    })?;
    if record.schema_version != TRANSCRIPT_SCHEMA_V2
        || record.kind != "tool"
        || binding.id != artifact_id
    {
        return Err(AppError::blocked(
            "tool-output view transcript/artifact id binding 불일치",
        ));
    }
    let path = validated_tool_output_path(
        &record.project_id,
        &record.session_id,
        &record.workflow_id,
        artifact_id,
        false,
    )?;
    let artifact = load_tool_output_artifact(&path)?;
    if artifact.content_hash != binding.hash
        || artifact.artifact_id != binding.id
        || artifact.project_id != record.project_id
        || artifact.session_id != record.session_id
        || artifact.workflow_id != record.workflow_id
        || artifact.tool_id != record.causal_id
    {
        return Err(AppError::blocked(
            "tool-output view canonical transcript/owner/hash binding 불일치",
        ));
    }
    Ok(tool_output_view(artifact))
}

fn tool_output_view(artifact: SanitizedToolOutputArtifact) -> ToolOutputView {
    ToolOutputView {
        artifact_id: artifact.artifact_id,
        session_id: artifact.session_id,
        workflow_id: artifact.workflow_id,
        tool_id: artifact.tool_id,
        created_at_ms: artifact.created_at_ms,
        stdout: artifact.stdout,
        stderr: artifact.stderr,
        stdout_truncated: artifact.stdout_truncated,
        stderr_truncated: artifact.stderr_truncated,
        stdout_redacted: artifact.stdout_redacted,
        stderr_redacted: artifact.stderr_redacted,
    }
}
