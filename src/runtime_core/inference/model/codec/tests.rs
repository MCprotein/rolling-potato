use std::path::Path;

use crate::runtime_core::inference::benchmark as benchmark_policy;

use super::super::manifest::PromotionEvidence;
use super::super::promotion::PromotionBenchmarkEvidence;
use super::{parse_registry_entry, render_promotion_evidence, render_registry_entry_snapshot};

#[test]
fn registry_v1_remains_text_ready_without_claiming_vision() {
    let text = r#"{
  "schemaVersion": 1,
  "id": "legacy",
  "displayName": "Legacy",
  "status": "installed",
  "evidenceStatus": "source-backed-manifest",
  "promotionEvidencePath": "",
  "backendVersion": "",
  "benchmarkRunId": "",
  "upstreamModel": "owner/model",
  "upstreamUrl": "https://example.com/model",
  "artifactPath": "/models/model.gguf",
  "artifactSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "licenseSource": "https://example.com/license",
  "licenseCheckedAt": "2026-07-23"
}"#;

    let entry = parse_registry_entry(text).unwrap();

    assert_eq!(entry.vision_status, "unavailable-legacy");
    assert!(entry.mmproj_path.is_none());
    assert!(entry.mmproj_sha256.is_none());
    assert!(entry.mmproj_size_bytes.is_none());
}

#[test]
fn model_upgrade_compatibility_v1_snapshot_migrates_without_losing_evidence() {
    let text = r#"{
  "schemaVersion": 1,
  "id": "legacy",
  "displayName": "Legacy",
  "status": "installed",
  "evidenceStatus": "verified-local-promotion",
  "promotionEvidencePath": "/models/evidence.json",
  "backendVersion": "b9878",
  "benchmarkRunId": "benchmark-1",
  "upstreamModel": "owner/model",
  "upstreamUrl": "https://example.com/model",
  "artifactPath": "/models/model.gguf",
  "artifactSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "licenseSource": "https://example.com/license",
  "licenseCheckedAt": "2026-07-23"
}"#;
    let mut entry = parse_registry_entry(text).unwrap();
    entry.vision_status = "ready".to_string();
    entry.mmproj_path = Some("/models/mmproj.gguf".to_string());
    entry.mmproj_sha256 =
        Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());
    entry.mmproj_size_bytes = Some(991);

    let migrated = parse_registry_entry(&render_registry_entry_snapshot(&entry)).unwrap();

    assert_eq!(migrated, entry);
}

#[test]
fn registry_v2_rejects_unbound_vision_ready_claims() {
    let text = r#"{
  "schemaVersion": 2,
  "id": "vision",
  "displayName": "Vision",
  "status": "installed",
  "evidenceStatus": "source-backed-manifest",
  "promotionEvidencePath": "",
  "backendVersion": "",
  "benchmarkRunId": "",
  "upstreamModel": "owner/model",
  "upstreamUrl": "https://example.com/model",
  "artifactPath": "/models/model.gguf",
  "artifactSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "visionStatus": "ready",
  "mmprojPath": "",
  "mmprojSha256": "",
  "mmprojSizeBytes": 0,
  "licenseSource": "https://example.com/license",
  "licenseCheckedAt": "2026-07-23"
}"#;

    assert!(parse_registry_entry(text).is_err());
}

#[test]
fn promotion_evidence_renderer_preserves_exact_bytes() {
    let candidate = &super::super::manifest::CANDIDATES[0];
    let evidence = PromotionEvidence {
        model_id: candidate.id.to_string(),
        artifact_sha256: "a".repeat(64),
        artifact_size_bytes: 123,
        backend_id: "llama.cpp".to_string(),
        backend_version: "b1".to_string(),
        backend_smoke_event_id: "event-1".to_string(),
        ram_fit: "observed-within-local-host".to_string(),
        recommended_ram_gb: 8,
        peak_rss_bytes: 456,
        mmproj: "not-required-text-only".to_string(),
        benchmark_run_id: "benchmark-1".to_string(),
        recorded_at: "2026-07-16".to_string(),
    };
    let benchmark = PromotionBenchmarkEvidence {
        claim_state: "measured-locally".to_string(),
        local_pass: Some(true),
        backend_id: Some("llama.cpp".to_string()),
        fixture_id: "fixture-1".to_string(),
        fixture_sha256: "b".repeat(64),
        prompt_artifact_sha256: Some("c".repeat(64)),
        evidence_schema_version: Some(benchmark_policy::BENCHMARK_EVIDENCE_SCHEMA_VERSION),
        generation_status: Some(
            crate::runtime_core::observability::facade::BenchmarkGenerationStatus::Complete,
        ),
        finish_reason: Some("stop".to_string()),
        generation_profile_fingerprint: Some(
            benchmark_policy::expected_generation_profile_fingerprint(
                "a".repeat(64).as_str(),
                candidate.generation_profile.unwrap(),
            ),
        ),
        benchmark_name: "local-smoke".to_string(),
        score: Some(3.0),
        dataset_ref: Some("dataset-1".to_string()),
        peak_rss_bytes: Some(456),
        model_run_id: Some("model-run-1".to_string()),
    };

    let rendered = render_promotion_evidence(
        candidate,
        &evidence,
        Path::new("/models/model.gguf"),
        &benchmark,
        Path::new("/evidence/source.json"),
    );

    assert_eq!(
        rendered,
        format!(
            "{{\n  \"schemaVersion\": 1,\n  \"status\": \"verified-local-promotion\",\n  \"modelId\": \"{}\",\n  \"displayName\": \"{}\",\n  \"artifactPath\": \"/models/model.gguf\",\n  \"artifactSha256\": \"{}\",\n  \"artifactSizeBytes\": 123,\n  \"backendId\": \"llama.cpp\",\n  \"backendVersion\": \"b1\",\n  \"backendSmokeEventId\": \"event-1\",\n  \"ramFit\": \"observed-within-local-host\",\n  \"recommendedRamGb\": 8,\n  \"peakRssBytes\": 456,\n  \"mmproj\": \"not-required-text-only\",\n  \"benchmarkRunId\": \"benchmark-1\",\n  \"benchmarkName\": \"local-smoke\",\n  \"benchmarkScore\": 3.000000,\n  \"benchmarkLocalPass\": true,\n  \"sourceEvidencePath\": \"/evidence/source.json\",\n  \"recordedAt\": \"2026-07-16\"\n}}\n",
            candidate.id,
            candidate.display_name,
            "a".repeat(64)
        )
    );
}
