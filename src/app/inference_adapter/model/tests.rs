use std::fs;

use super::*;
use crate::runtime_core::inference::model::codec::{
    parse_default_selection, parse_registry_entry, render_default_selection,
};
use crate::runtime_core::inference::model::manifest::DefaultSelection;
use crate::runtime_core::inference::model::promotion::validate_registry_manifest_binding;

#[test]
fn candidate_summary_reports_verified_count() {
    let summary = candidate_summary();
    assert!(summary.contains("3개 후보"));
    assert!(summary.contains("verified 0개"));
}

#[test]
fn first_run_options_expose_only_source_backed_facts_and_one_evidence_based_recommendation() {
    let options = setup_options();

    assert_eq!(options.len(), 2);
    assert!(options.iter().all(|option| option.download_bytes > 0));
    assert!(options.iter().all(|option| option.context_length.is_some()));
    assert!(options.iter().all(|option| option.ram == "미확정"));
    assert_eq!(
        options
            .iter()
            .filter(|option| option.evaluation_recommended)
            .map(|option| option.id.as_str())
            .collect::<Vec<_>>(),
        ["gemma-4-e4b"]
    );
    assert!(options
        .iter()
        .find(|option| option.id == "gemma-4-e4b")
        .unwrap()
        .note
        .contains("16 GB 적합성은 미확정"));
}

#[test]
fn manifest_validation_blocks_unverified_artifact_candidate() {
    let candidate = find_candidate("qwen3.5-4b").unwrap();
    let validation = validate_install_ready(candidate);

    assert!(!validation.ready);
    assert!(validation
        .blockers
        .iter()
        .any(|blocker| blocker.contains("verified")));
    assert!(validation
        .blockers
        .iter()
        .any(|blocker| blocker.contains("promotion evidence")));
    assert!(validation
        .blockers
        .iter()
        .any(|blocker| blocker.contains("RAM")));
}

#[test]
fn manifest_report_names_required_source_backed_fields() {
    let report = manifest_report();
    assert!(report.contains("artifactUrl"));
    assert!(report.contains("sha256"));
    assert!(report.contains("benchmark ledger"));
}

#[test]
fn download_plan_blocks_candidate_without_verified_artifact() {
    let report = download_plan_report("qwen3.5-4b").unwrap();
    assert!(report.contains("status: blocked"));
    assert!(report.contains("license source"));
}

#[test]
fn evaluation_fetch_accepts_source_backed_unverified_candidate() {
    let candidate = find_candidate("qwen3.5-4b").unwrap();
    let artifact = source_backed_artifact(candidate).unwrap();

    assert_eq!(artifact.provider, "unsloth/Qwen3.5-4B-GGUF");
    assert_eq!(artifact.file_name, "Qwen3.5-4B-Q4_K_M.gguf");
    assert!(checksum::is_valid_sha256(artifact.sha256));
}

#[test]
fn evaluation_fetch_blocks_candidate_without_artifact_source() {
    let err = source_backed_artifact(find_candidate("qwen3.5-9b").unwrap()).unwrap_err();

    assert_eq!(err.code, 3);
    assert!(err.message.contains("fetch 차단"));
    assert!(err.message.contains("artifact provider"));
}

#[test]
fn eval_plan_reports_missing_local_artifact_without_download() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let data_root =
        std::env::temp_dir().join(format!("rpotato-eval-plan-test-{}", std::process::id()));
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

    let report = eval_plan_report("qwen3.5-4b").unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");

    assert!(report.contains("blocked-before-backend-smoke"));
    assert!(report.contains("local artifact status: missing"));
    assert!(report.contains("local benchmark status: not-run"));
    assert!(report.contains("fetch-candidate qwen3.5-4b --for-evaluation"));
}

#[test]
fn registry_parser_accepts_pretty_json_entries() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let candidate = find_candidate("qwen3.5-4b").unwrap();
    let artifact = source_backed_artifact(candidate).unwrap();
    let text = registry_entry_json(candidate, None);
    let entry = parse_registry_entry(&text).unwrap();

    assert_eq!(entry.id, "qwen3.5-4b");
    assert_eq!(entry.status, "installed");
    assert_eq!(entry.vision_status, "unavailable");
    assert!(entry.mmproj_path.is_none());
    assert!(entry.artifact_sha256.starts_with("00fe"));
    validate_registry_manifest_binding(&entry, candidate, artifact, &model_artifact_path(artifact))
        .unwrap();

    for drifted in [
        text.replace(candidate.license.source, "https://invalid.example/license"),
        text.replace(candidate.license.checked_at, "1999-01-01"),
        text.replace(candidate.upstream_model, "invalid/model"),
        text.replace(candidate.upstream_url, "https://invalid.example/model"),
    ] {
        let entry = parse_registry_entry(&drifted).unwrap();
        assert!(validate_registry_manifest_binding(
            &entry,
            candidate,
            artifact,
            &model_artifact_path(artifact),
        )
        .is_err());
    }
}

#[test]
fn default_selection_parser_is_strict_and_round_trips() {
    let selection = DefaultSelection {
        model_id: "qwen3.5-4b".to_string(),
        artifact_sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4"
            .to_string(),
        selected_at_ms: 42,
    };
    let rendered = render_default_selection(&selection);
    assert_eq!(
        rendered,
        "{\n  \"schemaVersion\": 1,\n  \"modelId\": \"qwen3.5-4b\",\n  \"artifactSha256\": \"00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4\",\n  \"selectedAtMs\": 42\n}\n"
    );
    assert_eq!(parse_default_selection(&rendered).unwrap(), selection);
    assert!(parse_default_selection(
        r#"{"schemaVersion":1,"modelId":"qwen3.5-4b","artifactSha256":"x","selectedAtMs":42,"unknown":true}"#
    )
    .is_err());
}

#[test]
fn default_resolution_fails_closed_when_selection_is_missing() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let data_root =
        std::env::temp_dir().join(format!("rpotato-default-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&data_root);
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

    let error = default_artifact_path().unwrap_err();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(data_root);
    assert!(error.message.contains("기본 모델이 선택되지 않았습니다"));
}

#[test]
fn eval_plan_blocks_candidate_without_artifact_source() {
    let report = eval_plan_report("qwen3.5-9b").unwrap();

    assert!(report.contains("blocked-before-artifact-fetch"));
    assert!(report.contains("artifact provider"));
    assert!(report.contains("benchmark source"));
}

#[test]
fn benchmark_plan_separates_public_and_local_conditions() {
    let report = benchmark_plan_report("qwen3.5-4b").unwrap();

    assert!(report.contains("public benchmark parity status"));
    assert!(report.contains("blocked-until-conditions-fixed"));
    assert!(report.contains("local product benchmark suite"));
    assert!(report.contains("published-vs-local rule"));
}

#[test]
fn cleanup_failed_dry_run_lists_app_managed_paths() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let data_root =
        std::env::temp_dir().join(format!("rpotato-cleanup-test-{}", std::process::id()));
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

    let report = cleanup_failed_report("qwen3.5-4b", true).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    assert!(report.contains("dry-run"));
    assert!(report.contains("qwen3.5-4b.part"));
    assert!(report.contains("app data downloads/models"));
}
