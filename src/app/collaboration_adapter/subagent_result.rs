//! Filesystem artifact adapter for verified subagent results.

use super::subagent::{SubagentRecordV1, SubagentStatus};
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::context::ContextPack;
use crate::foundation::error::AppError;
#[cfg(test)]
use crate::runtime_core::collaboration::subagent_result::MAX_PATCH_TEXT_BYTES;
use crate::runtime_core::collaboration::subagent_result::{
    self as result_policy, evidence_id, evidence_source_bindings, has_artifact_id,
    installable_evidence_body, render_evidence_payload_v2, validate_context_binding,
    verify_evidence_artifact, EvidenceSourceBinding, ResultBinding, SourcePointerBinding,
};
pub(crate) use crate::runtime_core::collaboration::subagent_result::{
    SubagentResultV1, MAX_RESULT_BYTES,
};
use std::fs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSubagentResult {
    pub result: SubagentResultV1,
    pub result_artifact_id: String,
    pub result_artifact_hash: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    result_body: String,
    evidence_sources: Vec<EvidenceSourceBinding>,
}

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
    let result_body = state::read_regular_file_bounded(
        &paths::project_subagent_result_file(&record.result_artifact_id),
        MAX_RESULT_BYTES as u64,
        "subagent completed result artifact",
    )?;
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
    let installed_evidence = state::read_regular_file_bounded(
        &paths::project_evidence_dir().join(format!("{}.json", record.evidence_id)),
        MAX_RESULT_BYTES as u64,
        "subagent completed evidence artifact",
    )?;
    verify_evidence_artifact(record, &result, &installed_evidence)?;
    Ok(())
}

pub fn verify_completed_source_freshness(record: &SubagentRecordV1) -> Result<(), AppError> {
    verify_completed_artifacts(record)?;
    let result_body = state::read_regular_file_bounded(
        &paths::project_subagent_result_file(&record.result_artifact_id),
        MAX_RESULT_BYTES as u64,
        "subagent completed result artifact",
    )?;
    let result = parse_result_shape(record, &result_body)?;
    let installed_evidence = state::read_regular_file_bounded(
        &paths::project_evidence_dir().join(format!("{}.json", record.evidence_id)),
        MAX_RESULT_BYTES as u64,
        "subagent completed evidence artifact",
    )?;
    let Some(expected_sources) = verify_evidence_artifact(record, &result, &installed_evidence)?
    else {
        return Err(AppError::blocked(
            "subagent completed evidence source fingerprint binding 누락",
        ));
    };
    let current = crate::context::build_declared_context_pack(&record.read_paths)?;
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
    let body = state::read_regular_file_bounded(
        &paths::project_subagent_result_file(&record.result_artifact_id),
        MAX_RESULT_BYTES as u64,
        "subagent completed result artifact",
    )?;
    parse_result_shape(record, &body)
}

fn parse_result(
    record: &SubagentRecordV1,
    context: &ContextPack,
    body: &str,
) -> Result<SubagentResultV1, AppError> {
    let result = parse_result_shape(record, body)?;
    validate_context_binding(record, &result, &source_pointer_bindings(context))?;
    Ok(result)
}

fn source_pointer_bindings(context: &ContextPack) -> Vec<SourcePointerBinding<'_>> {
    context
        .source_pointers
        .iter()
        .map(|pointer| SourcePointerBinding {
            path: &pointer.path,
            stable_ref: &pointer.stable_ref,
            fingerprint: &pointer.fingerprint,
        })
        .collect()
}

fn parse_result_shape(record: &SubagentRecordV1, body: &str) -> Result<SubagentResultV1, AppError> {
    let result = result_policy::parse_result_shape(
        &ResultBinding {
            subagent_id: &record.subagent_id,
            parent_workflow_id: &record.parent_workflow_id,
            role: record.role,
        },
        body,
    )?;
    if result_text_fields(&result).any(ledger::contains_sensitive_text) {
        return Err(AppError::blocked("subagent result sensitive output 차단"));
    }
    Ok(result)
}

fn result_text_fields(result: &SubagentResultV1) -> impl Iterator<Item = &str> {
    std::iter::once(result.summary.as_str())
        .chain(result.findings.iter().map(String::as_str))
        .chain(result.validation_gaps.iter().map(String::as_str))
        .chain(std::iter::once(result.suggested_next_action.as_str()))
        .chain(
            result
                .patch_proposal
                .iter()
                .flat_map(|patch| [patch.find_text.as_str(), patch.replacement_text.as_str()]),
        )
}

fn install_exact_artifact(path: &std::path::Path, body: &str, label: &str) -> Result<(), AppError> {
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

#[cfg(test)]
mod tests {
    use super::super::subagent::validate_launch;
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn fixture(role: &str) -> (SubagentRecordV1, ContextPack) {
        fs::create_dir_all(paths::project_root().join("src")).unwrap();
        fs::write(
            paths::project_root().join("src/main.rs"),
            "fn main() { println!(\"old\"); }\n",
        )
        .unwrap();
        let tools = if role == "executor" {
            strings(&["read_file", "render_diff"])
        } else {
            strings(&["read_file"])
        };
        let writes = if role == "executor" {
            strings(&["src/main.rs"])
        } else {
            Vec::new()
        };
        let launch = validate_launch(
            role,
            "bounded task",
            &tools,
            &strings(&["src/main.rs"]),
            &writes,
            None,
            None,
        )
        .unwrap();
        let record = SubagentRecordV1::new(
            "project-test",
            "session-test",
            "workflow-test",
            1,
            &"a".repeat(64),
            launch,
        )
        .unwrap();
        let context = crate::context::build_declared_context_pack(&record.read_paths).unwrap();
        (record, context)
    }

    fn result_json(
        record: &SubagentRecordV1,
        context: &ContextPack,
        patch: Option<&str>,
    ) -> String {
        format!(
            "{{\"schema_version\":1,\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"role\":\"{}\",\"status\":\"completed\",\"summary\":\"완료 요약\",\"findings\":[\"확인 결과\"],\"patch_proposal\":{},\"evidence_refs\":[\"{}\"],\"validation_gaps\":[],\"suggested_next_action\":\"다음 단계\"}}",
            record.subagent_id,
            record.parent_workflow_id,
            record.role.as_str(),
            patch.unwrap_or("null"),
            context.source_pointers[0].stable_ref,
        )
    }

    #[test]
    fn strict_result_round_trips_to_deterministic_result_and_evidence() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let (record, context) = fixture("explore");
        let body = result_json(&record, &context, None);
        let first = parse_and_store(&record, &context, &body).unwrap();
        let second = parse_and_store(&record, &context, &body).unwrap();
        assert_eq!(first, second);
        assert!(paths::project_subagent_result_file(&first.result_artifact_id).is_file());
        assert!(paths::project_evidence_dir()
            .join(format!("{}.json", first.evidence_id))
            .is_file());
        verify_stored_artifacts(&record, &first).unwrap();
        fs::write(
            paths::project_evidence_dir().join(format!("{}.json", first.evidence_id)),
            "forged",
        )
        .unwrap();
        assert!(verify_stored_artifacts(&record, &first).is_err());
    }

    #[test]
    fn strict_result_rejects_unknown_missing_duplicate_invalid_and_identity_fields() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let (record, context) = fixture("explore");
        let valid = result_json(&record, &context, None);
        let unknown = valid.replacen("\"summary\":", "\"unknown\":0,\"summary\":", 1);
        assert!(parse_and_store(&record, &context, &unknown).is_err());
        let missing = valid.replacen("\"summary\":\"완료 요약\",", "", 1);
        assert!(parse_and_store(&record, &context, &missing).is_err());
        let duplicate = valid.replacen(
            "\"summary\":\"완료 요약\",",
            "\"summary\":\"완료 요약\",\"summary\":\"중복\",",
            1,
        );
        assert!(parse_and_store(&record, &context, &duplicate).is_err());
        let invalid = valid.replacen("완료 요약", "\\ud800", 1);
        assert!(parse_and_store(&record, &context, &invalid).is_err());
        let mismatched = valid.replacen(&record.subagent_id, "subagent-other", 1);
        assert!(parse_and_store(&record, &context, &mismatched).is_err());
    }

    #[test]
    fn sensitive_result_is_rejected_before_artifact_install() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let (record, context) = fixture("explore");
        let sensitive = result_json(&record, &context, None).replacen(
            "완료 요약",
            "token=SUPER_SECRET_SENTINEL",
            1,
        );

        let error = parse_and_store(&record, &context, &sensitive).unwrap_err();

        assert!(error.message.contains("sensitive output 차단"));
        assert!(!error.message.contains("SUPER_SECRET_SENTINEL"));
        assert!(!paths::project_subagent_results_dir().exists());
        assert!(!paths::project_evidence_dir().exists());
    }

    #[test]
    fn strict_result_enforces_exact_result_byte_maximum() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let (record, context) = fixture("executor");
        let find_text = "f".repeat(MAX_PATCH_TEXT_BYTES);
        let base_patch = format!(
            "{{\"target_path\":\"src/main.rs\",\"source_hash\":\"{}\",\"find_text\":\"{find_text}\",\"replacement_text\":\"\"}}",
            context.source_pointers[0].fingerprint
        );
        let base = result_json(&record, &context, Some(&base_patch));
        let replacement_len = MAX_RESULT_BYTES.checked_sub(base.len()).unwrap();
        assert!(replacement_len <= MAX_PATCH_TEXT_BYTES);
        let replacement_text = "r".repeat(replacement_len);
        let exact_patch = format!(
            "{{\"target_path\":\"src/main.rs\",\"source_hash\":\"{}\",\"find_text\":\"{find_text}\",\"replacement_text\":\"{replacement_text}\"}}",
            context.source_pointers[0].fingerprint
        );
        let exact = result_json(&record, &context, Some(&exact_patch));
        assert_eq!(exact.len(), MAX_RESULT_BYTES);
        assert!(parse_and_store(&record, &context, &exact).is_ok());

        let over_patch = format!(
            "{{\"target_path\":\"src/main.rs\",\"source_hash\":\"{}\",\"find_text\":\"{find_text}\",\"replacement_text\":\"{replacement_text}r\"}}",
            context.source_pointers[0].fingerprint
        );
        let over = result_json(&record, &context, Some(&over_patch));
        assert_eq!(over.len(), MAX_RESULT_BYTES + 1);
        assert!(parse_and_store(&record, &context, &over).is_err());
    }

    #[test]
    fn executor_patch_requires_declared_target_and_current_source_hash() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let (record, context) = fixture("executor");
        let source_hash = &context.source_pointers[0].fingerprint;
        let patch = format!(
            "{{\"target_path\":\"src/main.rs\",\"source_hash\":\"{source_hash}\",\"find_text\":\"old\",\"replacement_text\":\"new\"}}"
        );
        let valid = result_json(&record, &context, Some(&patch));
        assert!(parse_and_store(&record, &context, &valid).is_ok());

        let stale = valid.replacen(source_hash, &"b".repeat(64), 1);
        assert!(parse_and_store(&record, &context, &stale).is_err());
        let outside = valid.replacen("src/main.rs", "README.md", 1);
        assert!(parse_and_store(&record, &context, &outside).is_err());
    }

    #[test]
    fn non_executor_patch_and_undeclared_evidence_are_blocked() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let (record, context) = fixture("explore");
        let patch = format!(
            "{{\"target_path\":\"src/main.rs\",\"source_hash\":\"{}\",\"find_text\":\"old\",\"replacement_text\":\"new\"}}",
            context.source_pointers[0].fingerprint
        );
        assert!(parse_and_store(
            &record,
            &context,
            &result_json(&record, &context, Some(&patch))
        )
        .is_err());
        let undeclared = result_json(&record, &context, None).replacen(
            &context.source_pointers[0].stable_ref,
            "README.md:1",
            1,
        );
        assert!(parse_and_store(&record, &context, &undeclared).is_err());
    }
}
