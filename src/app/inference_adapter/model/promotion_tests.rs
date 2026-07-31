use std::fs;

use super::*;
use crate::app::observability_adapter as observability;
use crate::runtime_core::inference::benchmark as benchmark_policy;
use crate::runtime_core::inference::model::codec::{
    parse_promotion_evidence, parse_registry_entry,
};
use crate::runtime_core::inference::model::manifest::{
    BackendSmokeEvidence, LocalArtifactState, ModelArtifactDescriptor, PromotionEvidence,
};
use crate::runtime_core::inference::model::promotion::{
    artifact_model_id, measured_ram_budget_gb, validate_registry_promotion_binding, BYTES_PER_GIB,
};

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
        evidence_schema_version: Some(benchmark_policy::BENCHMARK_EVIDENCE_SCHEMA_VERSION),
        generation_status: Some(observability::BenchmarkGenerationStatus::Complete),
        finish_reason: Some("stop".to_string()),
        generation_profile_fingerprint: Some("status-only-test".to_string()),
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
fn promotion_evidence_rejects_legacy_and_profile_mismatched_benchmarks() {
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
            row.evidence_schema_version = None;
            row.generation_status = None;
            row.finish_reason = None;
            row.generation_profile_fingerprint = None;
            row
        },
        {
            let mut row = canonical;
            row.generation_profile_fingerprint = Some("stale-profile".to_string());
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
        assert!(validation.blockers.iter().any(|blocker| {
            blocker.contains("generation evidence schema")
                || blocker.contains("generation profile fingerprint")
        }));
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
        evidence_schema_version: Some(benchmark_policy::BENCHMARK_EVIDENCE_SCHEMA_VERSION),
        generation_status: Some(observability::BenchmarkGenerationStatus::Complete),
        finish_reason: Some("stop".to_string()),
        generation_profile_fingerprint: Some(
            benchmark_policy::expected_generation_profile_fingerprint(
                artifact.sha256,
                find_candidate("qwen3.5-4b")
                    .unwrap()
                    .generation_profile
                    .unwrap(),
            ),
        ),
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
    let sampling =
        crate::runtime_core::inference::model::manifest::generation_profile_for_artifact_hash(
            artifact.sha256,
        )
        .expect("Qwen fixture must have an exact generation profile")
        .1
        .sampling
        .map(|sampling| sampling.ledger_label())
        .unwrap_or_else(|| "model-default".to_string());
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
        sampling,
        host_os: "macos".to_string(),
        host_arch: "aarch64".to_string(),
    }
}
