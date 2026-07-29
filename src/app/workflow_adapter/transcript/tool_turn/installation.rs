use std::fs;
use std::path::Path;

use crate::adapters::filesystem::lease;
use crate::foundation::error::AppError;

use super::super::storage::{install_record, load_record_path, load_tool_output_artifact};
use super::types::PreparedTranscriptTurn;

pub(crate) fn install_prepared_no_stream_tool_turn(
    prepared: &PreparedTranscriptTurn,
) -> Result<(), AppError> {
    {
        let _tool_lock = lease::RecoverableLease::acquire(
            prepared.tool_path.with_extension("checkpoint.lock"),
            "tool-output artifact",
        )?;
        install_exact_artifact(&prepared.tool_path, &prepared.tool_bytes)?;
        let artifact = load_tool_output_artifact(&prepared.tool_path)?;
        if artifact.artifact_id != prepared.tool_artifact_id
            || artifact.to_json() != prepared.tool_bytes
        {
            return Err(AppError::blocked(
                "prepared tool-output installed bytes 불일치",
            ));
        }
    }
    {
        let _transcript_lock = lease::RecoverableLease::acquire(
            prepared.transcript_path.with_extension("checkpoint.lock"),
            "transcript checkpoint",
        )?;
        let installed_bytes = install_record(&prepared.transcript_path, &prepared.record)?;
        let record = load_record_path(&prepared.transcript_path)?;
        if installed_bytes != prepared.transcript_bytes
            || record != prepared.record
            || record.to_json() != prepared.transcript_bytes
        {
            return Err(AppError::blocked(
                "prepared TranscriptRecord installed bytes 불일치",
            ));
        }
    }
    Ok(())
}

fn install_exact_artifact(path: &Path, bytes: &str) -> Result<(), AppError> {
    if path.exists() {
        let existing = fs::read_to_string(path)
            .map_err(|err| AppError::blocked(format!("prepared artifact reread 실패: {err}")))?;
        if existing == bytes {
            return Ok(());
        }
        return Err(AppError::blocked("prepared artifact immutable conflict"));
    }
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(path, bytes.as_bytes())
}
