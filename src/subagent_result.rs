use crate::app::AppError;
use crate::context::ContextPack;
use crate::strict_json::{CanonicalObject, CanonicalValue};
use crate::subagent::{SubagentRecordV1, SubagentRole};
use crate::{ledger, paths, state, strict_json};
use std::collections::BTreeSet;
use std::fs;

pub const MAX_RESULT_BYTES: usize = 65_536;
const MAX_SUMMARY_BYTES: usize = 4_096;
const MAX_ITEM_BYTES: usize = 2_048;
const MAX_ITEMS: usize = 16;
const MAX_PATCH_TEXT_BYTES: usize = 32_768;
const RESULT_KEYS: &[&str] = &[
    "schema_version",
    "subagent_id",
    "parent_workflow_id",
    "role",
    "status",
    "summary",
    "findings",
    "patch_proposal",
    "evidence_refs",
    "validation_gaps",
    "suggested_next_action",
];
const PATCH_KEYS: &[&str] = &[
    "target_path",
    "source_hash",
    "find_text",
    "replacement_text",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentPatchProposalV1 {
    pub target_path: String,
    pub source_hash: String,
    pub find_text: String,
    pub replacement_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentResultV1 {
    pub subagent_id: String,
    pub parent_workflow_id: String,
    pub role: String,
    pub status: String,
    pub summary: String,
    pub findings: Vec<String>,
    pub patch_proposal: Option<SubagentPatchProposalV1>,
    pub evidence_refs: Vec<String>,
    pub validation_gaps: Vec<String>,
    pub suggested_next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSubagentResult {
    pub result: SubagentResultV1,
    pub result_artifact_id: String,
    pub result_artifact_hash: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    result_body: String,
}

pub fn parse_and_store(
    record: &SubagentRecordV1,
    context: &ContextPack,
    body: &str,
) -> Result<StoredSubagentResult, AppError> {
    if body.is_empty() || body.len() > MAX_RESULT_BYTES {
        return Err(AppError::blocked(format!(
            "subagent result byte 범위 오류: 1..={MAX_RESULT_BYTES}"
        )));
    }
    let result = parse_result(record, context, body)?;
    let result_artifact_hash = state::sha256_text(body);
    let result_artifact_id = format!("subagent-result-{}", &result_artifact_hash[..20]);
    install_exact_artifact(
        &paths::project_subagent_result_file(&result_artifact_id),
        body,
        "subagent result",
    )?;
    let evidence_id = format!(
        "evidence-subagent-{}",
        &state::sha256_text(&format!(
            "{}\n{}\n{}",
            record.subagent_id, record.parent_workflow_id, result_artifact_hash
        ))[..20]
    );
    let evidence_payload = render_evidence_payload(
        &evidence_id,
        record,
        &result_artifact_id,
        &result_artifact_hash,
        &result.evidence_refs,
    );
    let evidence_hash = state::sha256_text(&evidence_payload);
    let evidence_body = evidence_payload.replacen(
        "\"subagent_id\"",
        &format!("\"artifact_hash\":\"{}\",\"subagent_id\"", evidence_hash),
        1,
    );
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
    let evidence_payload = render_evidence_payload(
        &stored.evidence_id,
        record,
        &stored.result_artifact_id,
        &stored.result_artifact_hash,
        &stored.result.evidence_refs,
    );
    let evidence_body = evidence_payload.replacen(
        "\"subagent_id\"",
        &format!(
            "\"artifact_hash\":\"{}\",\"subagent_id\"",
            stored.evidence_hash
        ),
        1,
    );
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

fn parse_result(
    record: &SubagentRecordV1,
    context: &ContextPack,
    body: &str,
) -> Result<SubagentResultV1, AppError> {
    let object = strict_json::parse_canonical_object(body, RESULT_KEYS, "subagent result")?;
    if strict_json::canonical_u64(&object, "schema_version", "subagent result")? != 1 {
        return Err(AppError::blocked("subagent result schema version 불일치"));
    }
    let result = SubagentResultV1 {
        subagent_id: string(&object, "subagent_id")?,
        parent_workflow_id: string(&object, "parent_workflow_id")?,
        role: string(&object, "role")?,
        status: string(&object, "status")?,
        summary: string(&object, "summary")?,
        findings: string_array(&object, "findings")?,
        patch_proposal: patch(&object)?,
        evidence_refs: string_array(&object, "evidence_refs")?,
        validation_gaps: string_array(&object, "validation_gaps")?,
        suggested_next_action: string(&object, "suggested_next_action")?,
    };
    if result.subagent_id != record.subagent_id
        || result.parent_workflow_id != record.parent_workflow_id
        || result.role != record.role.as_str()
        || result.status != "completed"
    {
        return Err(AppError::blocked(
            "subagent result identity/status binding 불일치",
        ));
    }
    validate_bounded_text(&result.summary, "summary", 1, MAX_SUMMARY_BYTES)?;
    validate_bounded_text(
        &result.suggested_next_action,
        "suggested_next_action",
        0,
        MAX_ITEM_BYTES,
    )?;
    validate_items(&result.findings, "findings", false)?;
    validate_items(&result.evidence_refs, "evidence_refs", true)?;
    validate_items(&result.validation_gaps, "validation_gaps", false)?;
    let allowed_evidence = context
        .source_pointers
        .iter()
        .map(|pointer| pointer.stable_ref.as_str())
        .collect::<BTreeSet<_>>();
    if result
        .evidence_refs
        .iter()
        .any(|reference| !allowed_evidence.contains(reference.as_str()))
    {
        return Err(AppError::blocked(
            "subagent result evidence ref가 declared context binding 밖입니다.",
        ));
    }
    validate_patch(record, context, result.patch_proposal.as_ref())?;
    Ok(result)
}

fn validate_patch(
    record: &SubagentRecordV1,
    context: &ContextPack,
    patch: Option<&SubagentPatchProposalV1>,
) -> Result<(), AppError> {
    let Some(patch) = patch else {
        return Ok(());
    };
    if record.role != SubagentRole::Executor
        || !record
            .declared_tools
            .iter()
            .any(|tool| tool == "render_diff")
    {
        return Err(AppError::blocked(
            "executor/render_diff가 아닌 subagent patch proposal 차단",
        ));
    }
    let normalized = crate::subagent::normalize_relative_path(&patch.target_path)?;
    if normalized != patch.target_path
        || !record.read_paths.iter().any(|path| path == &normalized)
        || !record.write_paths.iter().any(|owner| {
            normalized == *owner
                || normalized
                    .strip_prefix(owner)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    {
        return Err(AppError::blocked(
            "subagent patch target declared read/write ownership 불일치",
        ));
    }
    let Some(pointer) = context
        .source_pointers
        .iter()
        .find(|pointer| pointer.path == normalized)
    else {
        return Err(AppError::blocked("subagent patch source context 누락"));
    };
    if patch.source_hash != pointer.fingerprint {
        return Err(AppError::blocked("subagent patch source hash 불일치"));
    }
    validate_bounded_text(&patch.find_text, "patch.find_text", 1, MAX_PATCH_TEXT_BYTES)?;
    validate_bounded_text(
        &patch.replacement_text,
        "patch.replacement_text",
        0,
        MAX_PATCH_TEXT_BYTES,
    )?;
    if patch.find_text == patch.replacement_text {
        return Err(AppError::blocked(
            "subagent patch proposal은 실제 변경이어야 합니다.",
        ));
    }
    Ok(())
}

fn patch(object: &CanonicalObject) -> Result<Option<SubagentPatchProposalV1>, AppError> {
    match object.get("patch_proposal") {
        Some(CanonicalValue::Null) => Ok(None),
        Some(CanonicalValue::Object(patch)) => {
            let actual = patch
                .entries
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>();
            if actual != PATCH_KEYS {
                return Err(AppError::blocked(
                    "subagent patch proposal exact key order 불일치",
                ));
            }
            Ok(Some(SubagentPatchProposalV1 {
                target_path: string(patch, "target_path")?,
                source_hash: string(patch, "source_hash")?,
                find_text: string(patch, "find_text")?,
                replacement_text: string(patch, "replacement_text")?,
            }))
        }
        _ => Err(AppError::blocked(
            "subagent result patch_proposal type 오류",
        )),
    }
}

fn string(object: &CanonicalObject, key: &str) -> Result<String, AppError> {
    match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "subagent result missing/wrong string: {key}"
        ))),
    }
}

fn string_array(object: &CanonicalObject, key: &str) -> Result<Vec<String>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get(key) else {
        return Err(AppError::blocked(format!(
            "subagent result missing/wrong array: {key}"
        )));
    };
    values
        .iter()
        .map(|value| match value {
            CanonicalValue::String(value) => Ok(value.clone()),
            _ => Err(AppError::blocked(format!(
                "subagent result array item type 오류: {key}"
            ))),
        })
        .collect()
}

fn validate_items(values: &[String], label: &str, required: bool) -> Result<(), AppError> {
    if values.len() > MAX_ITEMS || (required && values.is_empty()) {
        return Err(AppError::blocked(format!(
            "subagent result {label} count 범위 오류"
        )));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_bounded_text(value, label, 1, MAX_ITEM_BYTES)?;
        if !seen.insert(value) {
            return Err(AppError::blocked(format!(
                "subagent result {label} duplicate 차단"
            )));
        }
    }
    Ok(())
}

fn validate_bounded_text(
    value: &str,
    label: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), AppError> {
    if value.len() < minimum || value.len() > maximum || value.trim() != value {
        return Err(AppError::blocked(format!(
            "subagent result {label} byte/canonical 범위 오류"
        )));
    }
    Ok(())
}

fn render_evidence_payload(
    evidence_id: &str,
    record: &SubagentRecordV1,
    result_artifact_id: &str,
    result_artifact_hash: &str,
    evidence_refs: &[String],
) -> String {
    format!(
        "{{\"schema_version\":1,\"evidence_id\":\"{}\",\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"result_artifact_id\":\"{}\",\"result_artifact_hash\":\"{}\",\"evidence_refs\":{}}}",
        ledger::json_string(evidence_id),
        ledger::json_string(&record.subagent_id),
        ledger::json_string(&record.parent_workflow_id),
        ledger::json_string(result_artifact_id),
        result_artifact_hash,
        render_string_array(evidence_refs),
    )
}

fn render_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", ledger::json_string(value)))
            .collect::<Vec<_>>()
            .join(",")
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
    state::atomic_replace_bytes(path, body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subagent::validate_launch;

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
