use crate::app::workflow_adapter::ledger::{self, ParsedLedgerEvent};
use crate::foundation::error::AppError;
use crate::runtime_core::workflow::domain::transcript as transcript_domain;
use crate::runtime_core::workflow::storage_compat::transcript::{
    TranscriptRecord, TRANSCRIPT_SCHEMA_V2,
};

use super::storage::{load_record_path, validated_transcript_path};
use super::transcript_ledger_event;

pub fn records_for_session(session_id: &str) -> Result<Vec<TranscriptRecord>, AppError> {
    super::storage::validate_id("session id", session_id)?;
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
