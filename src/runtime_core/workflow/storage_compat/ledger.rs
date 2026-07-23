//! Canonical ledger DTOs, codecs, and hashing ownership.

use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeIdentity {
    pub project_id: String,
    pub session_id: String,
    pub project_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEvent {
    pub event_id: String,
    pub ts_ms: u128,
    pub event_type: String,
    pub project_id: String,
    pub session_id: String,
    pub summary: String,
    pub details: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLedgerEvent {
    pub event_id: String,
    pub ts_ms: u128,
    pub event_type: String,
    pub project_id: String,
    pub session_id: String,
    pub summary: String,
    pub details: String,
    pub previous_event_hash: Option<String>,
    pub event_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowCheckpoint {
    pub revision: u64,
    pub artifact_hash: String,
    pub previous_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerBinding {
    pub event_count: u64,
    pub event_id: Option<String>,
    pub event_hash: String,
}

#[cfg(test)]
pub fn parse_event_line(line: &str) -> Option<ParsedLedgerEvent> {
    parse_event_line_strict(line).ok()
}

pub(crate) fn parse_event_line_strict(line: &str) -> Result<ParsedLedgerEvent, AppError> {
    const KEYS: &[&str] = &[
        "schema_version",
        "event_id",
        "ts_ms",
        "event_type",
        "project_id",
        "session_id",
        "summary",
        "details",
        "previous_event_hash",
        "event_hash",
    ];
    let object = strict_json::parse_object(line, KEYS, "runtime ledger line")?;
    let schema = strict_json::number(&object, "schema_version", "runtime ledger line")?;
    if !matches!(schema, 1 | 2) {
        return Err(AppError::blocked("runtime ledger schema version 불일치"));
    }
    let (previous_event_hash, event_hash) = if schema == 2 {
        (
            Some(strict_json::string(
                &object,
                "previous_event_hash",
                "runtime ledger line",
            )?),
            Some(strict_json::string(
                &object,
                "event_hash",
                "runtime ledger line",
            )?),
        )
    } else {
        if object.contains_key("previous_event_hash") || object.contains_key("event_hash") {
            return Err(AppError::blocked("legacy ledger에 chain field가 존재함"));
        }
        (None, None)
    };
    Ok(ParsedLedgerEvent {
        event_id: strict_json::string(&object, "event_id", "runtime ledger line")?,
        ts_ms: strict_json::number_u128(&object, "ts_ms", "runtime ledger line")?,
        event_type: strict_json::string(&object, "event_type", "runtime ledger line")?,
        project_id: strict_json::string(&object, "project_id", "runtime ledger line")?,
        session_id: strict_json::string(&object, "session_id", "runtime ledger line")?,
        summary: strict_json::string(&object, "summary", "runtime ledger line")?,
        details: strict_json::string(&object, "details", "runtime ledger line")?,
        previous_event_hash,
        event_hash,
    })
}

pub(crate) fn event_chain_payload(event: &LedgerEvent, previous: &str) -> String {
    format!(
        "{{\"schema_version\":2,\"event_id\":\"{}\",\"ts_ms\":{},\"event_type\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"summary\":\"{}\",\"details\":\"{}\",\"previous_event_hash\":\"{}\"}}",
        json_string(&event.event_id), event.ts_ms, json_string(&event.event_type),
        json_string(&event.project_id), json_string(&event.session_id), json_string(&event.summary),
        json_string(&event.details), previous
    )
}

pub(crate) fn canonical_event_line(event: &LedgerEvent, previous: &str) -> (String, String) {
    let payload = event_chain_payload(event, previous);
    let event_hash = sha256_bytes(payload.as_bytes());
    let line = format!(
        "{{{},\"event_hash\":\"{}\"}}",
        payload.trim_start_matches('{').trim_end_matches('}'),
        event_hash
    );
    (line, event_hash)
}

pub(crate) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl LedgerEvent {
    #[cfg(test)]
    pub fn to_json_line(&self) -> String {
        format!(
            "{{\"schema_version\":1,\"event_id\":\"{}\",\"ts_ms\":{},\"event_type\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"summary\":\"{}\",\"details\":\"{}\"}}",
            json_string(&self.event_id),
            self.ts_ms,
            json_string(&self.event_type),
            json_string(&self.project_id),
            json_string(&self.session_id),
            json_string(&self.summary),
            json_string(&self.details)
        )
    }

    pub(crate) fn to_log_line(&self) -> String {
        format!(
            "{} {} {} {}",
            self.ts_ms, self.event_type, self.session_id, self.summary
        )
    }
}

pub fn json_string(value: &str) -> String {
    crate::foundation::serialization::escape_string_content(value)
}

pub(crate) fn planned_event_hash(event: &LedgerEvent, previous: &str) -> String {
    sha256_bytes(event_chain_payload(event, previous).as_bytes())
}

pub(crate) fn event_physical_hash(event: &ParsedLedgerEvent, previous: &str) -> String {
    let synthetic = LedgerEvent {
        event_id: event.event_id.clone(),
        ts_ms: event.ts_ms,
        event_type: event.event_type.clone(),
        project_id: event.project_id.clone(),
        session_id: event.session_id.clone(),
        summary: event.summary.clone(),
        details: event.details.clone(),
    };
    sha256_bytes(event_chain_payload(&synthetic, previous).as_bytes())
}
