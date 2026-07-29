//! Ontology persistence, seeding, and reporting application adapter.

use std::fs;

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
#[cfg(test)]
use crate::runtime_core::knowledge::ontology::validate_import_text;
use crate::runtime_core::knowledge::ontology::{layer_a_record, schema_body, OntologyRecord};

mod exchange;
mod lifecycle;
mod project_paths;
mod projection;
mod reporting;
mod seeding;
mod source_reader;

#[allow(unused_imports)]
pub(crate) use crate::runtime_core::knowledge::ontology::{
    OntologyExportFormat, OntologySeedOutcome, RuntimeContextSelection,
};
pub(crate) use exchange::{export_report, import_report};
pub(crate) use lifecycle::ensure_seeded;
use project_paths::{canonical_project_root, relative_to_root};
pub(crate) use projection::runtime_context;
pub(crate) use reporting::{
    context_report, doctor_summary, inspect_report, seed_report, status_report,
};
pub(crate) use source_reader::{reread_historical_source, reread_report, reread_runtime_source};

#[cfg(test)]
#[path = "ontology_adapter/tests.rs"]
mod tests;
