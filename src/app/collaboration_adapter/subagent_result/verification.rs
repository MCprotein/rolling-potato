use std::fs;

use super::super::subagent::{SubagentRecordV1, SubagentStatus};
use super::types::StoredSubagentResult;
use super::validation::parse_result_shape;
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent_result::{
    evidence_id, has_artifact_id, installable_evidence_body, render_evidence_payload_v2,
    verify_evidence_artifact, SubagentResultV1, MAX_RESULT_BYTES,
};

pub fn verify_stored_artifacts(
    record: &SubagentRecordV1,
    stored: &StoredSubagentResult,
) -> Result<(), AppError> {
    let result_body = fs::read_to_string(paths::project_subagent_result_file(
        &stored.result_artifact_id,
    ))
    .map_err(|err| AppError::blocked(format!("subagent result artifact 읽기 실패: {err}")))?;
    if result_body != stored.result_body
        || state::sha256_text(&result_body) != stored.result_artifact_hash
    {
        return Err(AppError::blocked(
            "subagent result artifact hash binding 불일치",
        ));
    }
    let evidence_payload = render_evidence_payload_v2(
        &stored.evidence_id,
        record,
        &stored.result_artifact_id,
        &stored.result_artifact_hash,
        &stored.result.evidence_refs,
        &stored.evidence_sources,
    );
    let evidence_body = installable_evidence_body(&evidence_payload, &stored.evidence_hash);
    let installed_evidence = fs::read_to_string(
        paths::project_evidence_dir().join(format!("{}.json", stored.evidence_id)),
    )
    .map_err(|err| AppError::blocked(format!("subagent evidence artifact 읽기 실패: {err}")))?;
    if installed_evidence != evidence_body
        || state::sha256_text(&evidence_payload) != stored.evidence_hash
    {
        return Err(AppError::blocked(
            "subagent evidence artifact hash binding 불일치",
        ));
    }
    Ok(())
}

pub fn verify_completed_artifacts(record: &SubagentRecordV1) -> Result<(), AppError> {
    if record.status != SubagentStatus::Completed
        || !has_artifact_id(&record.result_artifact_id, "subagent-result-")
        || !has_artifact_id(&record.evidence_id, "evidence-subagent-")
    {
        return Err(AppError::blocked(
            "subagent completed artifact/evidence binding 불일치",
        ));
    }
    let result_body = read_completed_result_body(record)?;
    let result_hash = state::sha256_text(&result_body);
    let expected_result_id = format!("subagent-result-{}", &result_hash[..20]);
    if result_hash != record.result_artifact_hash || expected_result_id != record.result_artifact_id
    {
        return Err(AppError::blocked(
            "subagent completed result artifact hash binding 불일치",
        ));
    }
    let result = parse_result_shape(record, &result_body)?;
    let expected_evidence_id = evidence_id(record, &result_hash);
    if expected_evidence_id != record.evidence_id {
        return Err(AppError::blocked(
            "subagent completed evidence identity binding 불일치",
        ));
    }
    let installed_evidence = read_completed_evidence(record)?;
    verify_evidence_artifact(record, &result, &installed_evidence)?;
    Ok(())
}

pub fn verify_completed_source_freshness(record: &SubagentRecordV1) -> Result<(), AppError> {
    verify_completed_artifacts(record)?;
    let result = parse_result_shape(record, &read_completed_result_body(record)?)?;
    let installed_evidence = read_completed_evidence(record)?;
    let Some(expected_sources) = verify_evidence_artifact(record, &result, &installed_evidence)?
    else {
        return Err(AppError::blocked(
            "subagent completed evidence source fingerprint binding 누락",
        ));
    };
    let current = crate::app::context_adapter::build_declared_context_pack(&record.read_paths)?;
    for expected in expected_sources {
        let Some(actual) = current
            .source_pointers
            .iter()
            .find(|pointer| pointer.stable_ref == expected.stable_ref)
        else {
            return Err(AppError::blocked(
                "subagent completed evidence source pointer 누락",
            ));
        };
        if actual.path != expected.path || actual.fingerprint != expected.fingerprint {
            return Err(AppError::blocked(format!(
                "subagent completed evidence source stale\n- source pointer: {}",
                expected.stable_ref
            )));
        }
    }
    Ok(())
}

pub fn load_completed_result(record: &SubagentRecordV1) -> Result<SubagentResultV1, AppError> {
    verify_completed_artifacts(record)?;
    parse_result_shape(record, &read_completed_result_body(record)?)
}

fn read_completed_result_body(record: &SubagentRecordV1) -> Result<String, AppError> {
    state::read_regular_file_bounded(
        &paths::project_subagent_result_file(&record.result_artifact_id),
        MAX_RESULT_BYTES as u64,
        "subagent completed result artifact",
    )
}

fn read_completed_evidence(record: &SubagentRecordV1) -> Result<String, AppError> {
    state::read_regular_file_bounded(
        &paths::project_evidence_dir().join(format!("{}.json", record.evidence_id)),
        MAX_RESULT_BYTES as u64,
        "subagent completed evidence artifact",
    )
}
