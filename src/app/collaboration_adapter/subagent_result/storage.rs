use std::fs;
use std::path::Path;

use super::super::subagent::SubagentRecordV1;
use super::types::StoredSubagentResult;
use super::validation::{parse_result, source_pointer_bindings};
use crate::adapters::filesystem::layout as paths;
use crate::app::context_adapter::ContextPack;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent_result::{
    evidence_id, evidence_source_bindings, installable_evidence_body, render_evidence_payload_v2,
};

pub fn parse_and_store(
    record: &SubagentRecordV1,
    context: &ContextPack,
    body: &str,
) -> Result<StoredSubagentResult, AppError> {
    let result = parse_result(record, context, body)?;
    let result_artifact_hash = state::sha256_text(body);
    let result_artifact_id = format!("subagent-result-{}", &result_artifact_hash[..20]);
    install_exact_artifact(
        &paths::project_subagent_result_file(&result_artifact_id),
        body,
        "subagent result",
    )?;
    let evidence_id = evidence_id(record, &result_artifact_hash);
    let sources = source_pointer_bindings(context);
    let evidence_sources = evidence_source_bindings(&sources, &result.evidence_refs)?;
    let evidence_payload = render_evidence_payload_v2(
        &evidence_id,
        record,
        &result_artifact_id,
        &result_artifact_hash,
        &result.evidence_refs,
        &evidence_sources,
    );
    let evidence_hash = state::sha256_text(&evidence_payload);
    let evidence_body = installable_evidence_body(&evidence_payload, &evidence_hash);
    install_exact_artifact(
        &paths::project_evidence_dir().join(format!("{evidence_id}.json")),
        &evidence_body,
        "subagent evidence",
    )?;
    Ok(StoredSubagentResult {
        result,
        result_artifact_id,
        result_artifact_hash,
        evidence_id,
        evidence_hash,
        result_body: body.to_string(),
        evidence_sources,
    })
}

fn install_exact_artifact(path: &Path, body: &str, label: &str) -> Result<(), AppError> {
    if path.exists() {
        let existing = fs::read_to_string(path)
            .map_err(|err| AppError::blocked(format!("{label} 기존 artifact 읽기 실패: {err}")))?;
        if existing != body {
            return Err(AppError::blocked(format!(
                "{label} deterministic artifact 충돌"
            )));
        }
        return Ok(());
    }
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(path, body.as_bytes())
}
