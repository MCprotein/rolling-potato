use std::collections::HashMap;

use super::{OntologyProjection, OntologyRecord};

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
