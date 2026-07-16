//! Validated transcript session and tool-output views.

use std::collections::BTreeSet;

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::ledger::{LedgerEvent, ParsedLedgerEvent};
use crate::runtime_core::workflow::storage_compat::transcript::{
    self as transcript_codec, TranscriptRecord, TRANSCRIPT_SCHEMA_V1, TRANSCRIPT_SCHEMA_V2,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutputView {
    pub artifact_id: String,
    pub session_id: String,
    pub workflow_id: String,
    pub tool_id: String,
    pub created_at_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptEventBinding {
    project_id: String,
    session_id: String,
    details: Vec<(String, String)>,
}

impl TranscriptEventBinding {
    pub(crate) fn record_id(&self) -> &str {
        self.detail("record_id")
            .expect("validated transcript event binding has record_id")
    }

    pub(crate) fn validate_record(&self, record: &TranscriptRecord) -> Result<(), AppError> {
        validate_event_detail_keys(&self.details, record.schema_version)?;
        let record_id = self.record_id();
        if record.record_id != record_id
            || record.project_id != self.project_id
            || record.session_id != self.session_id
            || self.detail("workflow_id") != Some(record.workflow_id.as_str())
            || self.detail("kind") != Some(record.kind.as_str())
            || self.detail("content_hash") != Some(record.content_hash.as_str())
            || self.detail("artifact_hash") != Some(record.artifact_hash.as_str())
        {
            return Err(AppError::blocked(format!(
                "transcript event binding 불일치\n- record id: {record_id}"
            )));
        }
        if record.schema_version == TRANSCRIPT_SCHEMA_V2 {
            let expected = record.tool_output_artifact.as_ref();
            for (key, actual) in [
                (
                    "tool_output_artifact_id",
                    self.detail("tool_output_artifact_id"),
                ),
                (
                    "tool_output_artifact_path",
                    self.detail("tool_output_artifact_path"),
                ),
                (
                    "tool_output_artifact_hash",
                    self.detail("tool_output_artifact_hash"),
                ),
            ] {
                let wanted = match (key, expected) {
                    ("tool_output_artifact_id", Some(binding)) => binding.id.as_str(),
                    ("tool_output_artifact_path", Some(binding)) => binding.path.as_str(),
                    ("tool_output_artifact_hash", Some(binding)) => binding.hash.as_str(),
                    (_, None) => "none",
                    _ => unreachable!(),
                };
                if actual != Some(wanted) {
                    return Err(AppError::blocked(format!(
                        "transcript event tool binding 불일치\n- record id: {record_id}"
                    )));
                }
            }
        }
        Ok(())
    }

    fn detail(&self, key: &str) -> Option<&str> {
        self.details
            .iter()
            .find_map(|(stored, value)| (stored == key).then_some(value.as_str()))
    }
}

pub(crate) fn collect_session_records(
    project_id: &str,
    session_id: &str,
    events: &[ParsedLedgerEvent],
    mut load_record: impl FnMut(&ParsedLedgerEvent) -> Result<TranscriptRecord, AppError>,
) -> Result<Vec<TranscriptRecord>, AppError> {
    let mut records = Vec::new();
    let mut seen = BTreeSet::new();
    for event in events {
        if event.project_id != project_id
            || event.session_id != session_id
            || event.event_type != "transcript.recorded"
        {
            continue;
        }
        let record = load_record(event)?;
        if !seen.insert(record.record_id.clone()) {
            return Err(AppError::blocked(format!(
                "transcript replay 차단\n- 이유: duplicate canonical record event\n- record id: {}",
                record.record_id
            )));
        }
        records.push(record);
    }
    Ok(records)
}

pub(crate) fn parse_event_binding(
    project_id: &str,
    session_id: &str,
    event_type: &str,
    details: &str,
) -> Result<TranscriptEventBinding, AppError> {
    if event_type != "transcript.recorded" {
        return Err(AppError::blocked("transcript event type 불일치"));
    }
    transcript_codec::validate_id("project id", project_id)?;
    transcript_codec::validate_id("session id", session_id)?;
    let details = parse_event_details(details)?
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    let record_id = detail_from_owned_pairs(&details, "record_id")
        .ok_or_else(|| AppError::blocked("transcript event field 누락: record_id"))?;
    transcript_codec::validate_id("record id", record_id)?;
    let expected_pointer = format!(
        "state/transcripts/{}/{}/{}.json",
        project_id, session_id, record_id
    );
    if detail_from_owned_pairs(&details, "artifact_pointer") != Some(expected_pointer.as_str()) {
        return Err(AppError::blocked(format!(
            "transcript event artifact pointer 불일치\n- record id: {record_id}"
        )));
    }
    Ok(TranscriptEventBinding {
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        details,
    })
}

pub(crate) fn validate_event_identity(
    event: &ParsedLedgerEvent,
    expected: &LedgerEvent,
    record_id: &str,
) -> Result<(), AppError> {
    if event.event_id != expected.event_id
        || event.ts_ms != expected.ts_ms
        || event.summary != expected.summary
    {
        return Err(AppError::blocked(format!(
            "transcript event identity/timestamp 불일치\n- record id: {record_id}"
        )));
    }
    Ok(())
}

pub(crate) fn parse_event_details(details: &str) -> Result<Vec<(&str, &str)>, AppError> {
    if details.is_empty()
        || details.trim() != details
        || details.contains("  ")
        || details.contains(['\n', '\r', '\t'])
    {
        return Err(AppError::blocked("transcript event details spacing 불일치"));
    }
    let mut pairs = Vec::new();
    for part in details.split(' ') {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| AppError::blocked("transcript event detail token 불일치"))?;
        if key.is_empty() || value.is_empty() || pairs.iter().any(|(stored, _)| *stored == key) {
            return Err(AppError::blocked(
                "transcript event detail key/value 불일치",
            ));
        }
        pairs.push((key, value));
    }
    Ok(pairs)
}

pub(crate) fn detail_from_pairs<'a>(pairs: &'a [(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find_map(|(stored, value)| (*stored == key).then_some(*value))
}

pub(crate) fn validate_event_details_for_schema(
    details: &str,
    schema_version: u64,
) -> Result<(), AppError> {
    let pairs = parse_event_details(details)?;
    let owned = pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    validate_event_detail_keys(&owned, schema_version)
}

fn detail_from_owned_pairs<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find_map(|(stored, value)| (stored == key).then_some(value.as_str()))
}

fn validate_event_detail_keys(
    details: &[(String, String)],
    schema_version: u64,
) -> Result<(), AppError> {
    let actual = details
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    let expected = match schema_version {
        TRANSCRIPT_SCHEMA_V1 => vec![
            "record_id",
            "workflow_id",
            "kind",
            "artifact_pointer",
            "artifact_hash",
            "content_hash",
        ],
        TRANSCRIPT_SCHEMA_V2 => vec![
            "record_id",
            "workflow_id",
            "kind",
            "artifact_pointer",
            "artifact_hash",
            "content_hash",
            "tool_output_artifact_id",
            "tool_output_artifact_path",
            "tool_output_artifact_hash",
        ],
        _ => return Err(AppError::blocked("transcript event schema 불일치")),
    };
    if actual != expected {
        return Err(AppError::blocked(
            "transcript event detail key/order 불일치",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(record_id: &str) -> TranscriptRecord {
        TranscriptRecord {
            schema_version: TRANSCRIPT_SCHEMA_V2,
            record_id: record_id.to_string(),
            project_id: "project-1".to_string(),
            session_id: "session-1".to_string(),
            workflow_id: "workflow-1".to_string(),
            kind: "user".to_string(),
            causal_id: "request-1".to_string(),
            content: "hello".to_string(),
            content_hash: "content-hash".to_string(),
            source_pointers: Vec::new(),
            recorded_at_ms: 7,
            tool_output_artifact: None,
            artifact_hash: "artifact-hash".to_string(),
        }
    }

    fn parsed_event(event_id: &str, record_id: &str) -> ParsedLedgerEvent {
        ParsedLedgerEvent {
            event_id: event_id.to_string(),
            ts_ms: 7,
            event_type: "transcript.recorded".to_string(),
            project_id: "project-1".to_string(),
            session_id: "session-1".to_string(),
            summary: "recorded".to_string(),
            details: format!("record_id={record_id}"),
            previous_event_hash: Some("root".to_string()),
            event_hash: Some(format!("hash-{event_id}")),
        }
    }

    fn event_details(record_id: &str) -> String {
        format!(
            "record_id={record_id} workflow_id=workflow-1 kind=user artifact_pointer=state/transcripts/project-1/session-1/{record_id}.json artifact_hash=artifact-hash content_hash=content-hash tool_output_artifact_id=none tool_output_artifact_path=none tool_output_artifact_hash=none"
        )
    }

    #[test]
    fn session_records_keep_ledger_order_and_reject_duplicates() {
        let events = vec![
            parsed_event("event-2", "record-2"),
            parsed_event("event-1", "record-1"),
        ];
        let records = collect_session_records("project-1", "session-1", &events, |event| {
            Ok(record(if event.event_id == "event-2" {
                "record-2"
            } else {
                "record-1"
            }))
        })
        .unwrap();
        assert_eq!(
            records
                .iter()
                .map(|record| record.record_id.as_str())
                .collect::<Vec<_>>(),
            vec!["record-2", "record-1"]
        );

        let duplicate = vec![
            parsed_event("event-1", "record-1"),
            parsed_event("event-2", "record-1"),
        ];
        let error = collect_session_records("project-1", "session-1", &duplicate, |_| {
            Ok(record("record-1"))
        })
        .unwrap_err();
        assert!(error.message.contains("duplicate canonical record event"));
    }

    #[test]
    fn event_binding_requires_exact_pointer_fields_and_order() {
        let record = record("record-1");
        let details = event_details(&record.record_id);
        let binding = parse_event_binding(
            &record.project_id,
            &record.session_id,
            "transcript.recorded",
            &details,
        )
        .unwrap();
        binding.validate_record(&record).unwrap();

        let reordered = details.replacen(
            "record_id=record-1 workflow_id=workflow-1",
            "workflow_id=workflow-1 record_id=record-1",
            1,
        );
        let binding = parse_event_binding(
            &record.project_id,
            &record.session_id,
            "transcript.recorded",
            &reordered,
        )
        .unwrap();
        let error = binding.validate_record(&record).unwrap_err();
        assert!(error.message.contains("key/order"));

        let bad_pointer = details.replace(
            "state/transcripts/project-1/session-1/record-1.json",
            "state/transcripts/project-1/session-2/record-1.json",
        );
        let error = parse_event_binding(
            &record.project_id,
            &record.session_id,
            "transcript.recorded",
            &bad_pointer,
        )
        .unwrap_err();
        assert!(error.message.contains("artifact pointer"));
    }

    #[test]
    fn event_identity_rejects_timestamp_or_digest_drift() {
        let event = parsed_event("event-1", "record-1");
        let expected = LedgerEvent {
            event_id: event.event_id.clone(),
            ts_ms: event.ts_ms,
            event_type: event.event_type.clone(),
            project_id: event.project_id.clone(),
            session_id: event.session_id.clone(),
            summary: event.summary.clone(),
            details: event.details.clone(),
        };
        validate_event_identity(&event, &expected, "record-1").unwrap();

        let mut drifted = expected;
        drifted.ts_ms += 1;
        let error = validate_event_identity(&event, &drifted, "record-1").unwrap_err();
        assert!(error.message.contains("identity/timestamp"));
    }
}
