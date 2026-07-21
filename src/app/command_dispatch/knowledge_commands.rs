//! CLI execution adapter for evidence and ontology commands.

use crate::app::evidence_adapter as evidence;
use crate::app::ontology_adapter as ontology;
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{EvidenceCommand, OntologyCommand};

pub(super) fn execute_evidence(command: EvidenceCommand) -> Result<(), AppError> {
    match command {
        EvidenceCommand::Validate { pointer } => {
            crate::surfaces::cli::render::emit_report(&evidence::validate_report(&pointer)?);
        }
    }
    Ok(())
}

pub(super) fn execute_ontology(command: OntologyCommand) -> Result<(), AppError> {
    match command {
        OntologyCommand::Status => {
            crate::surfaces::cli::render::emit_report(&ontology::status_report()?)
        }
        OntologyCommand::Seed => {
            crate::surfaces::cli::render::emit_report(&ontology::seed_report()?)
        }
        OntologyCommand::Inspect => {
            crate::surfaces::cli::render::emit_report(&ontology::inspect_report()?)
        }
        OntologyCommand::Context { query } => {
            crate::surfaces::cli::render::emit_report(&ontology::context_report(&query)?);
        }
        OntologyCommand::Reread { pointer } => {
            crate::surfaces::cli::render::emit_report(&ontology::reread_report(&pointer)?);
        }
        OntologyCommand::Export { format } => print!("{}", ontology::export_report(format)?),
        OntologyCommand::Import { path, dry_run } => {
            crate::surfaces::cli::render::emit_report(&ontology::import_report(&path, dry_run)?);
        }
    }
    Ok(())
}
