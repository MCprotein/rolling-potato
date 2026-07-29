use super::{
    OntologyDiagnostics, OntologyProjection, OntologyRecord, RuntimeContextRecord,
    RuntimeContextSelection, SOURCE_POINTER_NONE,
};

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
        super::codec::short_hash(&record.source_hash),
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
