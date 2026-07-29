use crate::adapters::filesystem::lease;
use crate::app::context_adapter::SourcePointer;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::transcript::{
    TranscriptRecord, TranscriptSourcePointer, MAX_TRANSCRIPT_CONTENT_BYTES, TRANSCRIPT_SCHEMA_V2,
};

use super::ensure_ledger_event_under_guard;
use super::owner::TranscriptOwner;
use super::storage::{
    install_record, load_record_path, now_ms, validate_expected_record, validate_id, validate_kind,
    validate_source_pointer, validate_tool_binding_for_record, validated_transcript_path,
};
use super::tool_turn::{record_tool_output_artifact, validate_requested_tool_streams};

pub fn record_workflow_turn(
    workflow: &state::WorkflowRecord,
    kind: &str,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
) -> Result<TranscriptRecord, AppError> {
    record_workflow_turn_with_streams(
        workflow,
        kind,
        causal_id,
        content,
        source_pointers,
        None,
        None,
    )
}

pub fn record_workflow_turn_with_streams(
    workflow: &state::WorkflowRecord,
    kind: &str,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
    stdout: Option<&str>,
    stderr: Option<&str>,
) -> Result<TranscriptRecord, AppError> {
    record_turn(
        &TranscriptOwner::for_workflow(workflow),
        Some(workflow),
        kind,
        causal_id,
        content,
        source_pointers,
        stdout,
        stderr,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_turn(
    owner: &TranscriptOwner,
    workflow: Option<&state::WorkflowRecord>,
    kind: &str,
    causal_id: &str,
    content: &str,
    source_pointers: &[SourcePointer],
    stdout: Option<&str>,
    stderr: Option<&str>,
) -> Result<TranscriptRecord, AppError> {
    validate_kind(kind)?;
    validate_id("project id", &owner.project_id)?;
    validate_id("transcript stream id", &owner.stream_id)?;
    validate_id("session id", &owner.session_id)?;
    validate_id("causal id", causal_id)?;
    if content.trim().is_empty() {
        return Err(AppError::blocked("transcript content가 비어 있습니다."));
    }
    if content.len() > MAX_TRANSCRIPT_CONTENT_BYTES {
        return Err(AppError::blocked(format!(
            "transcript content 저장 차단\n- 최대 UTF-8 byte 수: {MAX_TRANSCRIPT_CONTENT_BYTES}"
        )));
    }

    let record_id = format!(
        "transcript-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}\n{}\n{}",
            owner.project_id, owner.session_id, owner.stream_id, kind, causal_id
        ))[..24]
    );
    let ledger_guard = crate::app::workflow_adapter::ledger::LedgerWriterGuard::acquire()?;
    let path = validated_transcript_path(&owner.project_id, &owner.session_id, &record_id, true)?;
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

    if path.exists() {
        let existing = {
            let _lease = lease::RecoverableLease::acquire(
                path.with_extension("checkpoint.lock"),
                "transcript checkpoint",
            )?;
            load_record_path(&path)?
        };
        validate_expected_record(&existing, owner, kind, causal_id, content, &pointers)?;
        validate_requested_tool_streams(&existing, stdout, stderr)?;
        ensure_ledger_event_under_guard(&existing, &ledger_guard)?;
        return Ok(existing);
    }

    let tool_output_artifact = if kind == "tool" {
        let workflow = workflow.ok_or_else(|| {
            AppError::blocked("session transcript에는 tool stream을 기록할 수 없습니다.")
        })?;
        Some(record_tool_output_artifact(
            workflow, causal_id, stdout, stderr,
        )?)
    } else {
        if stdout.is_some() || stderr.is_some() {
            return Err(AppError::blocked(
                "non-tool transcript에는 tool stream을 바인딩할 수 없습니다.",
            ));
        }
        None
    };

    let record = {
        let _lease = lease::RecoverableLease::acquire(
            path.with_extension("checkpoint.lock"),
            "transcript checkpoint",
        )?;
        if path.exists() {
            let existing = load_record_path(&path)?;
            validate_expected_record(&existing, owner, kind, causal_id, content, &pointers)?;
            validate_requested_tool_streams(&existing, stdout, stderr)?;
            existing
        } else {
            let mut record = TranscriptRecord {
                schema_version: TRANSCRIPT_SCHEMA_V2,
                record_id,
                project_id: owner.project_id.clone(),
                session_id: owner.session_id.clone(),
                // The v2 storage field is retained for wire compatibility. It
                // identifies the transcript owner stream, which may be a
                // workflow or a session-scoped conversation.
                workflow_id: owner.stream_id.clone(),
                kind: kind.to_string(),
                causal_id: causal_id.to_string(),
                content: content.to_string(),
                content_hash: state::sha256_text(content),
                source_pointers: pointers,
                recorded_at_ms: now_ms(),
                tool_output_artifact,
                artifact_hash: String::new(),
            };
            validate_tool_binding_for_record(&record)?;
            record.artifact_hash = state::sha256_text(&record.artifact_payload());
            install_record(&path, &record)?;
            record
        }
    };
    ensure_ledger_event_under_guard(&record, &ledger_guard)?;
    Ok(record)
}
