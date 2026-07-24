use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::context_adapter::SourcePointer;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger::{self, ParsedLedgerEvent, RuntimeIdentity};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::domain::transcript as transcript_domain;
#[cfg(test)]
use crate::runtime_core::workflow::storage_compat::transcript::TRANSCRIPT_V2_KEYS;
use crate::runtime_core::workflow::storage_compat::transcript::{
    TranscriptRecord, TranscriptSourcePointer,
};
use crate::runtime_core::workflow::storage_compat::transcript::{
    MAX_TRANSCRIPT_CONTENT_BYTES, TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2,
};

mod owner;
mod storage;
mod tool_turn;

pub(crate) use owner::{record_session_turn, TranscriptOwner};
pub(crate) use tool_turn::{
    decode_prepared_no_stream_tool_turn, install_prepared_no_stream_tool_turn,
    prepare_no_stream_tool_turn, tool_output_view_from_canonical_record, PreparedTranscriptTurn,
};
use tool_turn::{record_tool_output_artifact, validate_requested_tool_streams};
#[cfg(test)]
use tool_turn::{sanitize_tool_stream, MAX_SANITIZED_STREAM_BYTES};

use storage::{
    detail_from_pairs, install_record, load_record_path, now_ms, parse_event_details,
    validate_event_details_for_schema, validate_expected_record, validate_id, validate_kind,
    validate_source_pointer, validate_tool_binding_for_record,
    validate_tool_binding_shape_for_record, validated_transcript_path,
};

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
fn record_turn(
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

pub fn records_for_session(session_id: &str) -> Result<Vec<TranscriptRecord>, AppError> {
    validate_id("session id", session_id)?;
    let identity = ledger::validated_current_identity()?;
    let events = ledger::read_runtime_events()?;
    transcript_domain::collect_session_records(
        &identity.project_id,
        session_id,
        &events,
        record_from_event,
    )
}

pub fn record_from_event(event: &ParsedLedgerEvent) -> Result<TranscriptRecord, AppError> {
    let record = record_from_binding(
        &event.project_id,
        &event.session_id,
        &event.event_type,
        &event.details,
    )?;
    if record.schema_version == TRANSCRIPT_SCHEMA_V2 {
        let expected = transcript_ledger_event(&record)?;
        transcript_domain::validate_event_identity(event, &expected, &record.record_id)?;
    }
    Ok(record)
}

pub fn record_from_binding(
    project_id: &str,
    session_id: &str,
    event_type: &str,
    details: &str,
) -> Result<TranscriptRecord, AppError> {
    let binding =
        transcript_domain::parse_event_binding(project_id, session_id, event_type, details)?;
    let record_id = binding.record_id();
    let path = validated_transcript_path(project_id, session_id, record_id, false)?;
    let record = load_record_path(&path)?;
    binding.validate_record(&record)?;
    Ok(record)
}

fn ensure_ledger_event_under_guard(
    record: &TranscriptRecord,
    guard: &crate::app::workflow_adapter::ledger::LedgerWriterGuard,
) -> Result<(), AppError> {
    if record.schema_version == TRANSCRIPT_SCHEMA_V1 {
        let existing = guard
            .events()?
            .into_iter()
            .enumerate()
            .filter(|(_, candidate)| {
                candidate.event_type == "transcript.recorded"
                    && parse_event_details(&candidate.details)
                        .ok()
                        .and_then(|pairs| {
                            detail_from_pairs(&pairs, "record_id").map(str::to_string)
                        })
                        .as_deref()
                        == Some(record.record_id.as_str())
            })
            .collect::<Vec<_>>();
        if existing.len() > 1 {
            return Err(AppError::blocked(format!(
                "duplicate legacy transcript event 차단\n- record id: {}",
                record.record_id
            )));
        }
        if let Some((index, existing)) = existing.first() {
            let event = crate::app::workflow_adapter::ledger::LedgerEvent {
                event_id: existing.event_id.clone(),
                ts_ms: existing.ts_ms,
                event_type: existing.event_type.clone(),
                project_id: existing.project_id.clone(),
                session_id: existing.session_id.clone(),
                summary: existing.summary.clone(),
                details: existing.details.clone(),
            };
            return observability::project_event_with_ordinal(
                &event,
                u64::try_from(index + 1)
                    .map_err(|_| AppError::blocked("legacy transcript ordinal overflow"))?,
            );
        }
        let identity = RuntimeIdentity {
            project_id: record.project_id.clone(),
            session_id: record.session_id.clone(),
            project_root: paths::project_root().display().to_string(),
        };
        let artifact_pointer = format!(
            "state/transcripts/{}/{}/{}.json",
            record.project_id, record.session_id, record.record_id
        );
        let event = ledger::new_event_for(
            &identity,
            "transcript.recorded",
            &format!("{} transcript record persisted", record.kind),
            &format!(
                "record_id={} workflow_id={} kind={} artifact_pointer={} artifact_hash={} content_hash={}",
                record.record_id,
                record.workflow_id,
                record.kind,
                artifact_pointer,
                record.artifact_hash,
                record.content_hash
            ),
        );
        let appended = guard.append_planned(&event)?;
        return observability::project_event_with_ordinal(&event, appended.ordinal);
    }
    let event = transcript_ledger_event(record)?;
    let existing = guard
        .events()?
        .into_iter()
        .filter(|candidate| {
            candidate.event_type == "transcript.recorded"
                && parse_event_details(&candidate.details)
                    .ok()
                    .and_then(|details| {
                        detail_from_pairs(&details, "record_id").map(str::to_string)
                    })
                    .as_deref()
                    == Some(record.record_id.as_str())
        })
        .collect::<Vec<_>>();
    if existing.len() > 1 {
        return Err(AppError::blocked(format!(
            "duplicate transcript ledger event 차단\n- record id: {}",
            record.record_id
        )));
    }
    if let Some(existing) = existing.first() {
        if existing.event_id != event.event_id
            || existing.ts_ms != event.ts_ms
            || existing.project_id != event.project_id
            || existing.session_id != event.session_id
            || existing.summary != event.summary
            || existing.details != event.details
        {
            return Err(AppError::blocked(format!(
                "transcript ledger event immutable binding 불일치\n- record id: {}",
                record.record_id
            )));
        }
        let ordinal = u64::try_from(
            guard
                .events()?
                .iter()
                .position(|candidate| candidate.event_id == event.event_id)
                .ok_or_else(|| AppError::blocked("transcript event ordinal 누락"))?
                + 1,
        )
        .map_err(|_| AppError::blocked("transcript event ordinal overflow"))?;
        return observability::project_event_with_ordinal(&event, ordinal);
    }
    let appended = guard.append_planned(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)
}

pub(super) fn transcript_ledger_event(
    record: &TranscriptRecord,
) -> Result<crate::app::workflow_adapter::ledger::LedgerEvent, AppError> {
    validate_tool_binding_shape_for_record(record)?;
    let identity = RuntimeIdentity {
        project_id: record.project_id.clone(),
        session_id: record.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    };
    let artifact_pointer = format!(
        "state/transcripts/{}/{}/{}.json",
        record.project_id, record.session_id, record.record_id
    );
    let details = match (record.schema_version, &record.tool_output_artifact) {
        (TRANSCRIPT_SCHEMA_V1, _) => format!(
            "record_id={} workflow_id={} kind={} artifact_pointer={} artifact_hash={} content_hash={}",
            record.record_id,
            record.workflow_id,
            record.kind,
            artifact_pointer,
            record.artifact_hash,
            record.content_hash
        ),
        (TRANSCRIPT_SCHEMA_V2, binding) => format!(
            "record_id={} workflow_id={} kind={} artifact_pointer={} artifact_hash={} content_hash={} tool_output_artifact_id={} tool_output_artifact_path={} tool_output_artifact_hash={}",
            record.record_id,
            record.workflow_id,
            record.kind,
            artifact_pointer,
            record.artifact_hash,
            record.content_hash,
            binding.as_ref().map(|value| value.id.as_str()).unwrap_or("none"),
            binding.as_ref().map(|value| value.path.as_str()).unwrap_or("none"),
            binding.as_ref().map(|value| value.hash.as_str()).unwrap_or("none")
        ),
        _ => return Err(AppError::blocked("transcript schema version 불일치")),
    };
    validate_event_details_for_schema(&details, record.schema_version)?;
    let pointer_hash = state::sha256_text(&record.source_pointers_json());
    let (tool_id, tool_path, tool_hash) = record
        .tool_output_artifact
        .as_ref()
        .map(|binding| {
            (
                binding.id.as_str(),
                binding.path.as_str(),
                binding.hash.as_str(),
            )
        })
        .unwrap_or(("", "", ""));
    let digest_input = [
        "rpotato.transcript-recorded-event-v1",
        &identity.project_id,
        &identity.session_id,
        &record.workflow_id,
        &record.record_id,
        &record.kind,
        &record.causal_id,
        &record.content_hash,
        &pointer_hash,
        tool_id,
        tool_path,
        tool_hash,
        &record.recorded_at_ms.to_string(),
        &record.artifact_hash,
    ]
    .join("\0");
    Ok(crate::app::workflow_adapter::ledger::LedgerEvent {
        event_id: format!("event-transcript-{}", state::sha256_text(&digest_input)),
        ts_ms: record.recorded_at_ms,
        event_type: "transcript.recorded".to_string(),
        project_id: identity.project_id,
        session_id: identity.session_id,
        summary: format!("{} transcript record persisted", record.kind),
        details,
    })
}

#[cfg(test)]
#[path = "transcript/tests.rs"]
mod tests;
