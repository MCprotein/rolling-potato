use std::fs;
use std::path::Path;

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::transcript::{
    self as transcript_codec, TranscriptRecord, TranscriptSourcePointer,
};

use super::super::TranscriptOwner;
use super::tool_artifact::validate_tool_binding_for_record;

pub(in super::super) fn load_record_path(path: &Path) -> Result<TranscriptRecord, AppError> {
    let body = fs::read_to_string(path).map_err(|err| {
        AppError::blocked(format!(
            "transcript artifact 읽기 실패\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    let record = parse_transcript_record_body(&body)?;
    validate_tool_binding_for_record(&record)?;
    Ok(record)
}

pub(in super::super) fn install_record(
    path: &Path,
    record: &TranscriptRecord,
) -> Result<String, AppError> {
    let existing = match fs::read_to_string(path) {
        Ok(body) => Some(body),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(AppError::blocked(format!(
                "TranscriptRecord canonical reread 실패: {err}"
            )))
        }
    };
    let install = transcript_codec::canonical_install_bytes(record, existing.as_deref())?;
    if let Some(bytes) = install {
        crate::adapters::filesystem::atomic_write::atomic_replace_bytes(path, bytes.as_bytes())?;
        Ok(bytes)
    } else {
        Ok(record.to_json())
    }
}

pub(in super::super) fn parse_transcript_record_body(
    body: &str,
) -> Result<TranscriptRecord, AppError> {
    transcript_codec::parse_record(body)
}

pub(in super::super) fn validate_expected_record(
    existing: &TranscriptRecord,
    owner: &TranscriptOwner,
    kind: &str,
    causal_id: &str,
    content: &str,
    pointers: &[TranscriptSourcePointer],
) -> Result<(), AppError> {
    if existing.project_id != owner.project_id
        || existing.session_id != owner.session_id
        || existing.workflow_id != owner.stream_id
        || existing.kind != kind
        || existing.causal_id != causal_id
        || existing.content != content
        || existing.source_pointers != pointers
    {
        return Err(AppError::blocked(format!(
            "transcript deterministic record 충돌\n- record id: {}",
            existing.record_id
        )));
    }
    Ok(())
}
