use std::fs;

use super::super::subagent::{validate_launch, SubagentRecordV1};
use super::{parse_and_store, verify_stored_artifacts};
use crate::adapters::filesystem::layout as paths;
use crate::app::context_adapter::ContextPack;
use crate::runtime_core::collaboration::subagent_result::{MAX_PATCH_TEXT_BYTES, MAX_RESULT_BYTES};

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
    let context =
        crate::app::context_adapter::build_declared_context_pack(&record.read_paths).unwrap();
    (record, context)
}

fn result_json(record: &SubagentRecordV1, context: &ContextPack, patch: Option<&str>) -> String {
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
