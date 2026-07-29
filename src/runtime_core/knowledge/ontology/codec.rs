use sha2::{Digest, Sha256};

use crate::foundation::serialization::escape_string_content;

use super::{OntologyRecord, SCHEMA_VERSION};

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

    pub(super) fn parse(line: &str) -> Option<Self> {
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

pub(super) fn stable_id(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    bytes_to_hex(&hasher.finalize())[..16].to_string()
}

pub(super) fn short_hash(value: &str) -> String {
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

pub(super) fn parse_json_string_tail(text: &str) -> Option<String> {
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
