use crate::adapters::filesystem::layout as paths;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger::{self, RuntimeIdentity};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::transcript::{
    TranscriptRecord, TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2,
};

use super::storage::{
    detail_from_pairs, parse_event_details, validate_event_details_for_schema,
    validate_tool_binding_shape_for_record,
};

pub(super) fn ensure_ledger_event_under_guard(
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
