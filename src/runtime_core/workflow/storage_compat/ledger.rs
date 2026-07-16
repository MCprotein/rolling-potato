//! Canonical ledger DTOs, codecs, hashing, and append ownership.

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
