use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::ontology::{
    diagnostics_from_projection, format_context_row, format_record_row, select_context_records,
};

use super::lifecycle::ensure_seeded;
use super::projection::{load_projection, record_source_is_stale};
use super::seeding::ensure_layout;

pub(crate) fn seed_report() -> Result<String, AppError> {
    let outcome = ensure_seeded()?;
    Ok(format!(
        "ontology seed 결과\n- store: {}\n- schema: {}\n- added records: {}\n- current records: {}\n- layer A facts: {}\n- layer B claims: {}\n- ledger event: {}\n- canonical: runtime typed graph JSONL\n- boundary: raw source text는 store에 장기 저장하지 않고 source pointer와 hash만 저장합니다.",
        outcome.store.display(),
        outcome.schema.display(),
        outcome.records_added,
        outcome.current_records,
        outcome.layer_a_records,
        outcome.layer_b_records,
        outcome.event_id
    ))
}

pub(crate) fn status_report() -> Result<String, AppError> {
    ensure_layout()?;
    let diagnostics = diagnostics_from_projection(&load_projection()?, record_source_is_stale);
    Ok(format!(
        "ontology status\n- store: {}\n- schema: {}\n- total records: {}\n- current projection: {}\n- layer A deterministic facts: {}\n- layer B semantic claims: {}\n- stale Layer A source hashes: {}\n- sourceless confirmed Layer B claims: {}\n- open questions: {}\n- compact context: `rpotato ontology context --query <text>`\n- source reread: `rpotato ontology reread <source-pointer>`\n- export views: json, jsonl\n- boundary: JSON/YAML/RDF/OWL은 inspection/export view이며 runtime source of truth는 이 typed graph store입니다.",
        paths::project_ontology_store_file().display(),
        paths::project_ontology_schema_file().display(),
        diagnostics.total_records,
        diagnostics.current_records,
        diagnostics.layer_a_records,
        diagnostics.layer_b_records,
        diagnostics.stale_layer_a,
        diagnostics.sourceless_confirmed_layer_b,
        diagnostics.open_questions
    ))
}

pub(crate) fn inspect_report() -> Result<String, AppError> {
    ensure_layout()?;
    let projection = load_projection()?;
    let diagnostics = diagnostics_from_projection(&projection, record_source_is_stale);
    let rows = projection
        .current_records
        .iter()
        .take(20)
        .map(format_record_row)
        .collect::<Vec<_>>()
        .join("\n");
    let rows = if rows.is_empty() {
        "- records: 없음; `rpotato ontology seed`를 실행하세요.".to_string()
    } else {
        rows
    };

    Ok(format!(
        "ontology inspect\n- current projection: {}\n- stale Layer A source hashes: {}\n- sourceless confirmed Layer B claims: {}\n{}\n- rule: compact view는 source pointer를 먼저 보여주며, patch 전에는 반드시 `ontology reread`로 원문을 다시 읽어야 합니다.",
        diagnostics.current_records,
        diagnostics.stale_layer_a,
        diagnostics.sourceless_confirmed_layer_b,
        rows
    ))
}

pub(crate) fn context_report(query: &str) -> Result<String, AppError> {
    if query.trim().is_empty() {
        return Err(AppError::usage(
            "ontology context에는 --query <text> 값이 필요합니다.",
        ));
    }

    ensure_layout()?;
    let projection = load_projection()?;
    let selected = select_context_records(&projection.current_records, query, 12);
    let rows = selected
        .iter()
        .map(format_context_row)
        .collect::<Vec<_>>()
        .join("\n");
    let rows = if rows.is_empty() {
        "- selected: 없음; 먼저 `rpotato ontology seed`로 Layer A fact를 생성하세요.".to_string()
    } else {
        rows
    };

    Ok(format!(
        "ontology context view\n- query: {}\n- selected records: {}\n- prompt rule: source-pointer-first, original-file reread before edits\n- raw source text stored: false\n{}\n- boundary: 이 출력은 small-model prompt용 compact view이며 canonical store 자체가 아닙니다.",
        query,
        selected.len(),
        rows
    ))
}

pub(crate) fn doctor_summary() -> String {
    let path = paths::project_ontology_store_file();
    if !path.exists() {
        return format!(
            "ontology store 미생성 ({}); rpotato init에서 준비",
            path.display()
        );
    }
    match load_projection() {
        Ok(projection) => {
            let diagnostics = diagnostics_from_projection(&projection, |_| false);
            format!(
                "ontology store {}, current {}, source hash audit deferred, sourceless confirmed Layer B {}",
                path.display(),
                diagnostics.current_records,
                diagnostics.sourceless_confirmed_layer_b
            )
        }
        Err(err) => format!("ontology 진단 실패: {}", err.message),
    }
}
