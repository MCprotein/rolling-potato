//! Typed ontology graph records and compact context projection.

use std::path::PathBuf;

#[path = "ontology/codec.rs"]
mod codec;
#[path = "ontology/import.rs"]
mod import;
#[path = "ontology/projection.rs"]
mod projection;
#[path = "ontology/selection.rs"]
mod selection;
#[cfg(test)]
#[path = "ontology/tests.rs"]
mod tests;

pub(crate) use import::validate_import_text;
pub(crate) use projection::{parse_projection, record_revision_pointer, seeded_record_changed};
pub(crate) use selection::{
    diagnostics_from_projection, format_context_row, format_record_row, runtime_context_selection,
    select_context_records,
};

pub(crate) const SCHEMA_VERSION: u32 = 1;
pub(crate) const SOURCE_POINTER_NONE: &str = "none";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OntologyExportFormat {
    Json,
    Jsonl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologySeedOutcome {
    pub store: PathBuf,
    pub schema: PathBuf,
    pub records_added: usize,
    pub current_records: usize,
    pub layer_a_records: usize,
    pub layer_b_records: usize,
    pub event_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContextRecord {
    pub id: String,
    pub layer: String,
    pub kind: String,
    pub label: String,
    pub source_pointer: String,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContextSelection {
    pub current_records: usize,
    pub selected: Vec<RuntimeContextRecord>,
    pub stale_rejected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSourceRead {
    pub relative_path: String,
    pub stable_ref: String,
    pub source_hash: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OntologyRecord {
    pub(crate) id: String,
    pub(crate) layer: String,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) status: String,
    pub(crate) claim_state: String,
    pub(crate) confidence: String,
    pub(crate) source_pointer: String,
    pub(crate) source_hash: String,
    pub(crate) evidence: String,
    pub(crate) supersedes: String,
    pub(crate) current: bool,
    pub(crate) event_id: String,
    pub(crate) created_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OntologyProjection {
    pub(crate) total_records: usize,
    pub(crate) current_records: Vec<OntologyRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OntologyDiagnostics {
    pub(crate) total_records: usize,
    pub(crate) current_records: usize,
    pub(crate) layer_a_records: usize,
    pub(crate) layer_b_records: usize,
    pub(crate) stale_layer_a: usize,
    pub(crate) sourceless_confirmed_layer_b: usize,
    pub(crate) open_questions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportValidation {
    pub(crate) records: usize,
}

pub(crate) fn schema_body() -> String {
    format!(
        "{{\n  \"schemaVersion\": {},\n  \"canonical\": \"runtime-typed-graph-jsonl\",\n  \"layers\": [\"A\", \"B\"],\n  \"claimStates\": [\"confirmed\", \"proposed\", \"weak\", \"superseded\", \"rejected\", \"open_question\"],\n  \"requiredSourceForConfirmedSemanticClaims\": true,\n  \"rawSourceRetention\": \"source-pointer-and-hash-only\"\n}}\n",
        SCHEMA_VERSION
    )
}

pub(crate) fn layer_a_record(
    kind: &str,
    label: &str,
    relative_path: &str,
    source_hash: &str,
    evidence: &str,
) -> OntologyRecord {
    OntologyRecord {
        id: format!("a:{kind}:{}", codec::stable_id(relative_path)),
        layer: "A".to_string(),
        kind: kind.to_string(),
        label: label.to_string(),
        status: "confirmed".to_string(),
        claim_state: "confirmed".to_string(),
        confidence: "1.00".to_string(),
        source_pointer: format!("{relative_path}:1"),
        source_hash: source_hash.to_string(),
        evidence: evidence.to_string(),
        supersedes: String::new(),
        current: true,
        event_id: "pending".to_string(),
        created_at_ms: 0,
    }
}
