use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::ontology::{
    diagnostics_from_projection, record_revision_pointer, seeded_record_changed,
    OntologySeedOutcome,
};

use super::projection::{load_projection, record_source_is_stale};
use super::seeding::{append_records, ensure_layout, seed_candidates};

pub(crate) fn ensure_seeded() -> Result<OntologySeedOutcome, AppError> {
    ensure_layout()?;
    let projection = load_projection()?;
    let candidates = seed_candidates()?;
    let existing_by_id = projection
        .current_records
        .iter()
        .map(|record| (record.id.clone(), record.clone()))
        .collect::<HashMap<_, _>>();

    let mut records_to_append = Vec::new();
    for mut candidate in candidates {
        match existing_by_id.get(&candidate.id) {
            Some(existing) if seeded_record_changed(existing, &candidate) => {
                candidate.supersedes = record_revision_pointer(existing);
                candidate.created_at_ms = now_ms();
                records_to_append.push(candidate);
            }
            Some(_) => {}
            None => {
                candidate.created_at_ms = now_ms();
                records_to_append.push(candidate);
            }
        }
    }

    let event_type = if records_to_append.is_empty() {
        "ontology.seed.noop"
    } else {
        "ontology.seed"
    };
    let event_id = state::record_event(
        event_type,
        "ontology Layer A seed",
        &format!(
            "store={} added_records={} canonical=typed-graph-jsonl",
            paths::project_ontology_store_file().display(),
            records_to_append.len()
        ),
    )?;

    for record in &mut records_to_append {
        record.event_id = event_id.clone();
    }
    append_records(&records_to_append)?;

    let projection = load_projection()?;
    let diagnostics = diagnostics_from_projection(&projection, record_source_is_stale);

    Ok(OntologySeedOutcome {
        store: paths::project_ontology_store_file(),
        schema: paths::project_ontology_schema_file(),
        records_added: records_to_append.len(),
        current_records: diagnostics.current_records,
        layer_a_records: diagnostics.layer_a_records,
        layer_b_records: diagnostics.layer_b_records,
        event_id,
    })
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
