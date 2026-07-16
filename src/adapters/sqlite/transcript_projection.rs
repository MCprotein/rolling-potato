//! SQLite projection of validated canonical transcript records.

use rusqlite::{params, Connection};

use crate::foundation::error::AppError;

pub(crate) struct TranscriptProjectionEvent<'a> {
    pub project_id: &'a str,
    pub session_id: &'a str,
    pub event_type: &'a str,
    pub details: &'a str,
    pub ledger_event_id: &'a str,
    pub event_ordinal: i64,
}

pub(crate) fn project_event(
    connection: &Connection,
    event: TranscriptProjectionEvent<'_>,
) -> Result<(), AppError> {
    if event.event_type != "transcript.recorded" {
        return Ok(());
    }
    let record = crate::transcript::record_from_binding(
        event.project_id,
        event.session_id,
        event.event_type,
        event.details,
    )?;
    let artifact_pointer = format!(
        "state/transcripts/{}/{}/{}.json",
        record.project_id, record.session_id, record.record_id
    );
    connection
        .execute(
            "INSERT OR REPLACE INTO transcript_records (
                record_id, session_id, workflow_id, ledger_event_id, event_ordinal,
                record_kind, causal_id,
                content, content_hash, source_pointers_json, artifact_pointer,
                artifact_hash, recorded_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.record_id,
                record.session_id,
                record.workflow_id,
                event.ledger_event_id,
                event.event_ordinal,
                record.kind,
                record.causal_id,
                record.content,
                record.content_hash,
                record.source_pointers_json(),
                artifact_pointer,
                record.artifact_hash,
                i64::try_from(record.recorded_at_ms).unwrap_or(i64::MAX)
            ],
        )
        .map_err(|err| AppError::runtime(format!("transcript projection 저장 실패: {err}")))?;
    Ok(())
}
