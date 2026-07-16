//! Typed ontology graph records and compact context projection.

use std::collections::HashMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::foundation::error::AppError;
use crate::foundation::serialization::escape_string_content;

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
        id: format!("a:{kind}:{}", stable_id(relative_path)),
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

pub(crate) fn parse_projection(contents: &str) -> OntologyProjection {
    let mut latest_by_id = HashMap::new();
    let mut total_records = 0;
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        total_records += 1;
        if let Some(record) = OntologyRecord::parse(line) {
            latest_by_id.insert(record.id.clone(), record);
        }
    }

    let mut current_records = latest_by_id
        .into_values()
        .filter(|record| record.current)
        .collect::<Vec<_>>();
    current_records.sort_by(|a, b| a.id.cmp(&b.id));
    OntologyProjection {
        total_records,
        current_records,
    }
}

pub(crate) fn seeded_record_changed(existing: &OntologyRecord, candidate: &OntologyRecord) -> bool {
    existing.layer != candidate.layer
        || existing.kind != candidate.kind
        || existing.label != candidate.label
        || existing.status != candidate.status
        || existing.claim_state != candidate.claim_state
        || existing.source_pointer != candidate.source_pointer
        || existing.source_hash != candidate.source_hash
        || existing.evidence != candidate.evidence
}

pub(crate) fn record_revision_pointer(record: &OntologyRecord) -> String {
    format!(
        "{}@{}",
        record.id,
        if record.event_id.is_empty() {
            record.created_at_ms.to_string()
        } else {
            record.event_id.clone()
        }
    )
}

pub(crate) fn diagnostics_from_projection(
    projection: &OntologyProjection,
    mut source_is_stale: impl FnMut(&OntologyRecord) -> bool,
) -> OntologyDiagnostics {
    let layer_a_records = projection
        .current_records
        .iter()
        .filter(|record| record.layer == "A")
        .count();
    let layer_b_records = projection
        .current_records
        .iter()
        .filter(|record| record.layer == "B")
        .count();
    let stale_layer_a = projection
        .current_records
        .iter()
        .filter(|record| record.layer == "A" && source_is_stale(record))
        .count();
    let sourceless_confirmed_layer_b = projection
        .current_records
        .iter()
        .filter(|record| semantic_claim_is_sourceless_confirmed(record))
        .count();
    let open_questions = projection
        .current_records
        .iter()
        .filter(|record| record.status == "open_question" || record.claim_state == "open_question")
        .count();

    OntologyDiagnostics {
        total_records: projection.total_records,
        current_records: projection.current_records.len(),
        layer_a_records,
        layer_b_records,
        stale_layer_a,
        sourceless_confirmed_layer_b,
        open_questions,
    }
}

pub(crate) fn select_context_records(
    records: &[OntologyRecord],
    query: &str,
    limit: usize,
) -> Vec<OntologyRecord> {
    let terms = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    let mut scored = records
        .iter()
        .map(|record| {
            let haystack = format!(
                "{} {} {} {} {}",
                record.id, record.kind, record.label, record.evidence, record.source_pointer
            )
            .to_ascii_lowercase();
            let score = terms
                .iter()
                .filter(|term| haystack.contains(term.as_str()))
                .count();
            (score, record)
        })
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();
    scored.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.layer.cmp(&right.layer))
            .then_with(|| left.id.cmp(&right.id))
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(_, record)| record.clone())
        .collect()
}

pub(crate) fn runtime_context_selection(
    projection: &OntologyProjection,
    query: &str,
    limit: usize,
    mut source_is_stale: impl FnMut(&OntologyRecord) -> bool,
) -> RuntimeContextSelection {
    let mut selected = select_context_records(&projection.current_records, query, limit);
    if selected.is_empty() {
        selected = projection
            .current_records
            .iter()
            .filter(|record| {
                record.layer == "A"
                    && matches!(
                        record.kind.as_str(),
                        "entrypoint" | "package-manager" | "file"
                    )
            })
            .take(limit)
            .cloned()
            .collect();
    }

    let mut stale_rejected = 0;
    let selected = selected
        .into_iter()
        .filter_map(|record| {
            if source_is_stale(&record) {
                stale_rejected += 1;
                return None;
            }
            Some(RuntimeContextRecord {
                id: record.id,
                layer: record.layer,
                kind: record.kind,
                label: record.label,
                source_pointer: record.source_pointer,
                source_hash: record.source_hash,
            })
        })
        .collect();

    RuntimeContextSelection {
        current_records: projection.current_records.len(),
        selected,
        stale_rejected,
    }
}

pub(crate) fn format_record_row(record: &OntologyRecord) -> String {
    format!(
        "- [{}:{}:{}] {} | source {} | hash {} | id {}",
        record.layer,
        record.kind,
        record.claim_state,
        record.label,
        record.source_pointer,
        short_hash(&record.source_hash),
        record.id
    )
}

pub(crate) fn format_context_row(record: &OntologyRecord) -> String {
    format!(
        "- source={} | {}:{}:{} | {} | id={}",
        record.source_pointer,
        record.layer,
        record.kind,
        record.claim_state,
        record.label,
        record.id
    )
}

pub(crate) fn validate_import_text(text: &str) -> Result<ImportValidation, AppError> {
    let schema_version = extract_json_u64_tolerant(text, "schemaVersion").ok_or_else(|| {
        AppError::usage("ontology import file에는 schemaVersion: 1이 필요합니다.")
    })?;
    if schema_version != u64::from(SCHEMA_VERSION) {
        return Err(AppError::usage(format!(
            "ontology import schemaVersion은 {}이어야 합니다: {}",
            SCHEMA_VERSION, schema_version
        )));
    }

    let mut records = 0;
    for line in text.lines().filter(|line| line.contains("\"id\"")) {
        records += 1;
        let layer = extract_json_string_tolerant(line, "layer").unwrap_or_default();
        let status = extract_json_string_tolerant(line, "status").unwrap_or_default();
        let claim_state = extract_json_string_tolerant(line, "claimState").unwrap_or_default();
        let source_pointer =
            extract_json_string_tolerant(line, "sourcePointer").unwrap_or_default();
        let source_hash = extract_json_string_tolerant(line, "sourceHash").unwrap_or_default();
        if layer == "B"
            && (status == "confirmed" || claim_state == "confirmed")
            && (source_pointer.trim().is_empty()
                || source_pointer == SOURCE_POINTER_NONE
                || source_hash.trim().is_empty())
        {
            return Err(AppError::blocked(
                "ontology import 차단: confirmed Layer B semantic claim에는 sourcePointer와 sourceHash가 필요합니다.",
            ));
        }
    }

    if records == 0 {
        records = text.matches("\"schemaVersion\"").count().saturating_sub(1);
    }
    Ok(ImportValidation { records })
}

impl OntologyRecord {
    pub(crate) fn to_json_line(&self) -> String {
        format!(
            "{{\"schemaVersion\":{},\"id\":\"{}\",\"layer\":\"{}\",\"kind\":\"{}\",\"label\":\"{}\",\"status\":\"{}\",\"claimState\":\"{}\",\"confidence\":\"{}\",\"sourcePointer\":\"{}\",\"sourceHash\":\"{}\",\"evidence\":\"{}\",\"supersedes\":\"{}\",\"current\":{},\"eventId\":\"{}\",\"createdAtMs\":{}}}",
            SCHEMA_VERSION,
            escape_string_content(&self.id),
            escape_string_content(&self.layer),
            escape_string_content(&self.kind),
            escape_string_content(&self.label),
            escape_string_content(&self.status),
            escape_string_content(&self.claim_state),
            escape_string_content(&self.confidence),
            escape_string_content(&self.source_pointer),
            escape_string_content(&self.source_hash),
            escape_string_content(&self.evidence),
            escape_string_content(&self.supersedes),
            self.current,
            escape_string_content(&self.event_id),
            self.created_at_ms
        )
    }

    fn parse(line: &str) -> Option<Self> {
        let schema_version = extract_json_u64(line, "schemaVersion")?;
        if schema_version != u64::from(SCHEMA_VERSION) {
            return None;
        }
        Some(Self {
            id: extract_json_string(line, "id")?,
            layer: extract_json_string(line, "layer")?,
            kind: extract_json_string(line, "kind")?,
            label: extract_json_string(line, "label")?,
            status: extract_json_string(line, "status")?,
            claim_state: extract_json_string(line, "claimState")?,
            confidence: extract_json_string(line, "confidence")?,
            source_pointer: extract_json_string(line, "sourcePointer")?,
            source_hash: extract_json_string(line, "sourceHash")?,
            evidence: extract_json_string(line, "evidence")?,
            supersedes: extract_json_string(line, "supersedes").unwrap_or_default(),
            current: extract_json_bool(line, "current").unwrap_or(true),
            event_id: extract_json_string(line, "eventId").unwrap_or_default(),
            created_at_ms: extract_json_u128(line, "createdAtMs").unwrap_or_default(),
        })
    }
}

fn semantic_claim_is_sourceless_confirmed(record: &OntologyRecord) -> bool {
    if record.layer != "B" {
        return false;
    }
    if record.status != "confirmed" && record.claim_state != "confirmed" {
        return false;
    }
    record.source_pointer.trim().is_empty()
        || record.source_pointer == SOURCE_POINTER_NONE
        || record.source_hash.trim().is_empty()
}

fn stable_id(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    bytes_to_hex(&hasher.finalize())[..16].to_string()
}

fn short_hash(value: &str) -> String {
    if value.len() <= 12 {
        value.to_string()
    } else {
        value[..12].to_string()
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn extract_json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = text.find(&needle)? + needle.len();
    parse_json_string_tail(&text[start..])
}

fn extract_json_string_tolerant(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    parse_json_string_tail(rest)
}

fn parse_json_string_tail(text: &str) -> Option<String> {
    let mut value = String::new();
    let mut escaped = false;
    for ch in text.chars() {
        if escaped {
            match ch {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => value.push(other),
            }
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}

fn extract_json_u64(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn extract_json_u64_tolerant(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start().strip_prefix(':')?.trim_start();
    rest.chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn extract_json_u128(text: &str, key: &str) -> Option<u128> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn extract_json_bool(text: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    if text[start..].starts_with("true") {
        Some(true)
    } else if text[start..].starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ontology_record_bytes_are_stable() {
        let record = OntologyRecord {
            id: "a:file:fixture".to_string(),
            layer: "A".to_string(),
            kind: "file".to_string(),
            label: "fixture".to_string(),
            status: "confirmed".to_string(),
            claim_state: "confirmed".to_string(),
            confidence: "1.00".to_string(),
            source_pointer: "src/main.rs:1".to_string(),
            source_hash: "source-hash".to_string(),
            evidence: "indexed-file".to_string(),
            supersedes: String::new(),
            current: true,
            event_id: "event-fixture".to_string(),
            created_at_ms: 42,
        };

        assert_eq!(
            record.to_json_line(),
            "{\"schemaVersion\":1,\"id\":\"a:file:fixture\",\"layer\":\"A\",\"kind\":\"file\",\"label\":\"fixture\",\"status\":\"confirmed\",\"claimState\":\"confirmed\",\"confidence\":\"1.00\",\"sourcePointer\":\"src/main.rs:1\",\"sourceHash\":\"source-hash\",\"evidence\":\"indexed-file\",\"supersedes\":\"\",\"current\":true,\"eventId\":\"event-fixture\",\"createdAtMs\":42}"
        );
    }

    #[test]
    fn projection_keeps_latest_current_record_and_context_binding() {
        let mut first = layer_a_record("file", "first", "src/main.rs", "old", "main");
        first.created_at_ms = 1;
        let mut latest = layer_a_record("file", "latest", "src/main.rs", "new", "main");
        latest.created_at_ms = 2;
        let contents = format!("{}\n{}\n", first.to_json_line(), latest.to_json_line());

        let projection = parse_projection(&contents);
        let selected = runtime_context_selection(&projection, "main", 4, |_| false);

        assert_eq!(projection.total_records, 2);
        assert_eq!(projection.current_records.len(), 1);
        assert_eq!(projection.current_records[0].label, "latest");
        assert_eq!(selected.selected[0].source_hash, "new");
    }
}
