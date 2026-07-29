use std::fs;

use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::ontology::{
    validate_import_text, OntologyExportFormat, SCHEMA_VERSION,
};

use super::projection::load_projection;
use super::seeding::ensure_layout;
use super::source_reader::resolve_project_relative_file;

pub(crate) fn export_report(format: OntologyExportFormat) -> Result<String, AppError> {
    ensure_layout()?;
    match format {
        OntologyExportFormat::Jsonl => {
            let contents =
                fs::read_to_string(paths::project_ontology_store_file()).map_err(|err| {
                    AppError::runtime(format!(
                        "ontology store를 읽지 못했습니다: {} ({err})",
                        paths::project_ontology_store_file().display()
                    ))
                })?;
            Ok(contents)
        }
        OntologyExportFormat::Json => {
            let projection = load_projection()?;
            let records = projection
                .current_records
                .iter()
                .map(|record| format!("    {}", record.to_json_line()))
                .collect::<Vec<_>>()
                .join(",\n");
            Ok(format!(
                "{{\n  \"schemaVersion\": {},\n  \"viewAuthority\": \"inspection-only\",\n  \"canonicalStore\": \"{}\",\n  \"records\": [\n{}\n  ]\n}}\n",
                SCHEMA_VERSION,
                ledger::json_string(&paths::project_ontology_store_file().display().to_string()),
                records
            ))
        }
    }
}

pub(crate) fn import_report(path: &str, dry_run: bool) -> Result<String, AppError> {
    if !dry_run {
        return Err(AppError::blocked(
            "ontology import는 현재 --dry-run만 허용합니다. 외부 view를 canonical store로 바로 승격하지 않습니다.",
        ));
    }

    let path = resolve_project_relative_file(path)?;
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "ontology import file을 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let validation = validate_import_text(&contents)?;

    Ok(format!(
        "ontology import dry-run 결과\n- file: {}\n- schemaVersion: {}\n- inspected records: {}\n- sourceless confirmed Layer B claims: 0\n- mutation: 없음\n- boundary: import file은 inspection/migration 후보이며 canonical store로 자동 승격하지 않습니다.",
        path.display(),
        SCHEMA_VERSION,
        validation.records
    ))
}
