use std::fs;

use super::*;
use crate::app::observability_adapter as observability;
use crate::runtime_core::inference::benchmark as benchmark_policy;
use crate::runtime_core::inference::model::codec::{
    parse_default_selection, parse_promotion_evidence, parse_registry_entry,
    render_default_selection,
};
use crate::runtime_core::inference::model::manifest::{
    BackendSmokeEvidence, DefaultSelection, LocalArtifactState, ModelArtifactDescriptor,
    PromotionEvidence,
};
use crate::runtime_core::inference::model::promotion::{
    artifact_model_id, measured_ram_budget_gb, validate_registry_manifest_binding,
    validate_registry_promotion_binding, BYTES_PER_GIB,
};

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
            .filter(|option| option.recommended)
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
fn evaluation_fetch_paths_stay_under_app_data() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let data_root =
        std::env::temp_dir().join(format!("rpotato-fetch-path-test-{}", std::process::id()));
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

    let candidate = find_candidate("gemma-4-e4b").unwrap();
    let artifact = source_backed_artifact(candidate).unwrap();
    let final_path = model_artifact_path(artifact);
    let part_path = model_artifact_part_path(candidate);

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");

    assert!(final_path.starts_with(data_root.join("models")));
    assert!(part_path.starts_with(data_root.join("downloads")));
    assert!(part_path.ends_with("gemma-4-e4b.part"));
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
fn local_benchmark_status_reports_measured_qwen_row() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let data_root = std::env::temp_dir().join(format!(
        "rpotato-benchmark-status-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&data_root);
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

    observability::record_benchmark_run(&observability::BenchmarkRunMetric {
        benchmark_run_id: "benchmark-qwen-smoke".to_string(),
        session_id: "session-test".to_string(),
        model_run_id: Some("model-run-test".to_string()),
        model_id: "Qwen3.5-4B-Q4_K_M".to_string(),
        benchmark_name: benchmark_policy::ADOPTION_BENCHMARK_NAME.to_string(),
        fixture_id: "executable-smoke".to_string(),
        fixture_sha256: "fixture-sha".to_string(),
        prompt_artifact_sha256: Some("prompt-sha".to_string()),
        prompt_chars: Some(147),
        claim_state: "measured-locally".to_string(),
        score: Some(3.0),
        score_unit: Some("0-3-local-product-score".to_string()),
        local_pass: Some(true),
        expected_matches: Some(1),
        expected_total: Some(1),
        forbidden_matches: Some(0),
        harness_ref: "rpotato-benchmark-harness@test".to_string(),
        dataset_ref: Some("local-executable-smoke".to_string()),
        backend_id: Some("llama.cpp".to_string()),
        latency_ms: Some(243.0),
        tokens_per_second: Some(28.8),
        prompt_tokens: Some(76),
        completion_tokens: Some(7),
        total_tokens: Some(83),
        resource_pressure: Some("normal".to_string()),
        peak_rss_bytes: Some(3_351_363_584),
        reproducibility_manifest: "{}".to_string(),
        redacted_report: "{}".to_string(),
        recorded_at_ms: 1000,
    })
    .unwrap();

    let artifact = source_backed_artifact(find_candidate("qwen3.5-4b").unwrap()).unwrap();
    let status = local_benchmark_status(artifact).unwrap();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(data_root);

    assert!(status.contains("measured-locally"));
    assert!(status.contains("latest_run=benchmark-qwen-smoke"));
    assert!(status.contains("score=3.000000"));
    assert!(status.contains("local_pass=true"));
}

#[test]
fn promotion_evidence_parser_accepts_pretty_json() {
    let text = r#"{
  "schemaVersion": 1,
  "modelId": "qwen3.5-4b",
  "artifactSha256": "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
  "artifactSizeBytes": 2740937888,
  "backendId": "llama.cpp",
  "backendVersion": "b9878",
  "backendSmokeEventId": "event-backend-chat",
  "ramFit": "observed-within-local-host",
  "recommendedRamGb": 6,
  "peakRssBytes": 3351363584,
  "mmproj": "not-required-text-only",
  "benchmarkRunId": "benchmark-local",
  "recordedAt": "2026-07-10T00:00:00Z"
}"#;

    let evidence = parse_promotion_evidence(text).unwrap();

    assert_eq!(evidence.model_id, "qwen3.5-4b");
    assert_eq!(evidence.backend_version, "b9878");
    assert_eq!(evidence.recommended_ram_gb, 6);
}

#[test]
fn promotion_evidence_validation_accepts_measured_local_benchmark() {
    let candidate = find_candidate("qwen3.5-4b").unwrap();
    let artifact = source_backed_artifact(candidate).unwrap();
    let evidence = qwen_promotion_evidence(artifact);
    let benchmark = qwen_benchmark_report(artifact, &evidence);
    let benchmark_evidence = promotion_benchmark_evidence(&benchmark);
    let smoke = qwen_backend_smoke(artifact, &evidence);
    let local_state = LocalArtifactState {
        status: "verified-local-artifact",
        detail: "test artifact verified".to_string(),
        verified: true,
    };

    let validation = validate_promotion_evidence(
        candidate,
        &evidence,
        artifact,
        &local_state,
        Some(&benchmark_evidence),
        Some(&smoke),
    );

    assert!(validation.ready, "{:?}", validation.blockers);
}

#[test]
fn promotion_evidence_validation_blocks_ram_and_benchmark_gaps() {
    let candidate = find_candidate("qwen3.5-4b").unwrap();
    let artifact = source_backed_artifact(candidate).unwrap();
    let mut evidence = qwen_promotion_evidence(artifact);
    evidence.ram_fit = "unknown".to_string();
    evidence.peak_rss_bytes = 20 * BYTES_PER_GIB;
    let local_state = LocalArtifactState {
        status: "verified-local-artifact",
        detail: "test artifact verified".to_string(),
        verified: true,
    };

    let validation =
        validate_promotion_evidence(candidate, &evidence, artifact, &local_state, None, None);

    assert!(!validation.ready);
    assert!(validation
        .blockers
        .iter()
        .any(|blocker| blocker.contains("ramFit")));
    assert!(validation
        .blockers
        .iter()
        .any(|blocker| blocker.contains("recommendedRamGb")));
    assert!(validation
        .blockers
        .iter()
        .any(|blocker| blocker.contains("benchmarkRunId")));
    assert!(validation
        .blockers
        .iter()
        .any(|blocker| blocker.contains("smoke event")));
}

#[test]
fn promotion_evidence_rejects_canonical_benchmark_contract_drift() {
    let candidate = find_candidate("qwen3.5-4b").unwrap();
    let artifact = source_backed_artifact(candidate).unwrap();
    let evidence = qwen_promotion_evidence(artifact);
    let smoke = qwen_backend_smoke(artifact, &evidence);
    let local_state = LocalArtifactState {
        status: "verified-local-artifact",
        detail: "test artifact verified".to_string(),
        verified: true,
    };
    let canonical = qwen_benchmark_report(artifact, &evidence);

    for benchmark in [
        {
            let mut row = canonical.clone();
            row.fixture_sha256 = "a".repeat(64);
            row
        },
        {
            let mut row = canonical.clone();
            row.prompt_artifact_sha256 = Some("b".repeat(64));
            row
        },
        {
            let mut row = canonical.clone();
            row.benchmark_name = "easier-smoke".to_string();
            row
        },
    ] {
        let benchmark_evidence = promotion_benchmark_evidence(&benchmark);
        let validation = validate_promotion_evidence(
            candidate,
            &evidence,
            artifact,
            &local_state,
            Some(&benchmark_evidence),
            Some(&smoke),
        );
        assert!(!validation.ready);
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("canonical model adoption smoke")));
    }
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
fn registry_promotion_binding_rejects_backend_and_benchmark_drift() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let candidate = find_candidate("qwen3.5-4b").unwrap();
    let artifact = source_backed_artifact(candidate).unwrap();
    let evidence = qwen_promotion_evidence(artifact);
    let text = registry_entry_json(candidate, Some(&evidence));
    let entry = parse_registry_entry(&text).unwrap();

    validate_registry_promotion_binding(
        &entry,
        &promotion_evidence_path(candidate.id),
        Some(&evidence),
    )
    .unwrap();
    for drifted in [
        text.replace(&evidence.backend_version, "b0000"),
        text.replace(&evidence.benchmark_run_id, "benchmark-drifted"),
    ] {
        let entry = parse_registry_entry(&drifted).unwrap();
        assert!(validate_registry_promotion_binding(
            &entry,
            &promotion_evidence_path(candidate.id),
            Some(&evidence),
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

fn qwen_promotion_evidence(artifact: ModelArtifactDescriptor) -> PromotionEvidence {
    PromotionEvidence {
        model_id: "qwen3.5-4b".to_string(),
        artifact_sha256: artifact.sha256.to_string(),
        artifact_size_bytes: artifact.size_bytes,
        backend_id: "llama.cpp".to_string(),
        backend_version: "b9878".to_string(),
        backend_smoke_event_id: "event-backend-chat".to_string(),
        ram_fit: "observed-within-local-host".to_string(),
        recommended_ram_gb: measured_ram_budget_gb(3_351_363_584),
        peak_rss_bytes: 3_351_363_584,
        mmproj: "not-required-text-only".to_string(),
        benchmark_run_id: "benchmark-local".to_string(),
        recorded_at: "2026-07-10T00:00:00Z".to_string(),
    }
}

fn qwen_benchmark_report(
    artifact: ModelArtifactDescriptor,
    evidence: &PromotionEvidence,
) -> observability::BenchmarkRunReport {
    observability::BenchmarkRunReport {
        benchmark_run_id: evidence.benchmark_run_id.clone(),
        session_id: "session-test".to_string(),
        model_run_id: Some(format!("model-run-{}", evidence.backend_smoke_event_id)),
        model_id: artifact_model_id(artifact),
        benchmark_name: benchmark_policy::ADOPTION_BENCHMARK_NAME.to_string(),
        fixture_id: benchmark_policy::ADOPTION_FIXTURE_ID.to_string(),
        fixture_sha256: benchmark_policy::ADOPTION_FIXTURE_SHA256.to_string(),
        prompt_artifact_sha256: Some(benchmark_policy::ADOPTION_PROMPT_SHA256.to_string()),
        prompt_chars: Some(147),
        claim_state: "measured-locally".to_string(),
        score: Some(3.0),
        score_unit: Some("0-3-local-product-score".to_string()),
        local_pass: Some(true),
        expected_matches: Some(1),
        expected_total: Some(1),
        forbidden_matches: Some(0),
        harness_ref: "rpotato-benchmark-harness@test".to_string(),
        dataset_ref: Some(benchmark_policy::ADOPTION_DATASET_REF.to_string()),
        backend_id: Some("llama.cpp".to_string()),
        latency_ms: Some(243.0),
        tokens_per_second: Some(28.8),
        prompt_tokens: Some(76),
        completion_tokens: Some(7),
        total_tokens: Some(83),
        resource_pressure: Some("normal".to_string()),
        peak_rss_bytes: Some(evidence.peak_rss_bytes),
        reproducibility_manifest: "{}".to_string(),
        redacted_report: "{}".to_string(),
        recorded_at_ms: 1000,
    }
}

fn qwen_backend_smoke(
    artifact: ModelArtifactDescriptor,
    evidence: &PromotionEvidence,
) -> BackendSmokeEvidence {
    BackendSmokeEvidence {
        event_id: evidence.backend_smoke_event_id.clone(),
        backend_id: "llama.cpp".to_string(),
        backend_release: evidence.backend_version.clone(),
        binary_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string(),
        model_id: artifact_model_id(artifact),
        model_sha256: artifact.sha256.to_string(),
        model_size_bytes: artifact.size_bytes,
        ctx_size: "4096".to_string(),
        mmproj: evidence.mmproj.clone(),
        sampling: "temperature-0.1_top-p-0.8".to_string(),
        host_os: "macos".to_string(),
        host_arch: "aarch64".to_string(),
    }
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
