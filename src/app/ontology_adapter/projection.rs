use std::fs;

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::ontology::{
    parse_projection, runtime_context_selection, OntologyProjection, OntologyRecord,
    RuntimeContextSelection,
};

use super::seeding::ensure_layout;
use super::source_reader::source_is_stale;

pub(super) fn load_projection() -> Result<OntologyProjection, AppError> {
    let path = paths::project_ontology_store_file();
    if !path.exists() {
        return Ok(OntologyProjection {
            total_records: 0,
            current_records: Vec::new(),
        });
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "ontology store를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(parse_projection(&contents))
}

pub(super) fn record_source_is_stale(record: &OntologyRecord) -> bool {
    source_is_stale(&record.source_pointer, &record.source_hash)
}

pub(crate) fn runtime_context(
    query: &str,
    limit: usize,
) -> Result<RuntimeContextSelection, AppError> {
    ensure_layout()?;
    let projection = load_projection()?;
    Ok(runtime_context_selection(
        &projection,
        query,
        limit,
        record_source_is_stale,
    ))
}
