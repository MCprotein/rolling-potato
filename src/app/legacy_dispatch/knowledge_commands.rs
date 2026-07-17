//! CLI execution adapter for evidence and ontology commands.

use crate::evidence;
use crate::foundation::error::AppError;
use crate::ontology;
use crate::surfaces::cli::command::{EvidenceCommand, OntologyCommand};

pub(super) fn execute_evidence(command: EvidenceCommand) -> Result<(), AppError> {
    match command {
        EvidenceCommand::Validate { pointer } => {
            println!("{}", evidence::validate_report(&pointer)?);
        }
    }
    Ok(())
}

pub(super) fn execute_ontology(command: OntologyCommand) -> Result<(), AppError> {
    match command {
        OntologyCommand::Status => println!("{}", ontology::status_report()?),
        OntologyCommand::Seed => println!("{}", ontology::seed_report()?),
        OntologyCommand::Inspect => println!("{}", ontology::inspect_report()?),
        OntologyCommand::Context { query } => {
            println!("{}", ontology::context_report(&query)?);
        }
        OntologyCommand::Reread { pointer } => {
            println!("{}", ontology::reread_report(&pointer)?);
        }
        OntologyCommand::Export { format } => print!("{}", ontology::export_report(format)?),
        OntologyCommand::Import { path, dry_run } => {
            println!("{}", ontology::import_report(&path, dry_run)?);
        }
    }
    Ok(())
}
