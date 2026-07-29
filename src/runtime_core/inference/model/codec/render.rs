use std::path::Path;

use crate::foundation::serialization as strict_json;

use super::super::manifest::{
    DefaultSelection, ModelManifestEntry, PromotionEvidence, RegistryEntry, RegistryVisionState,
};
use super::super::promotion::PromotionBenchmarkEvidence;

pub(crate) fn render_default_selection(selection: &DefaultSelection) -> String {
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"modelId\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"selectedAtMs\": {}\n}}\n",
        strict_json::escape_string_content(&selection.model_id),
        strict_json::escape_string_content(&selection.artifact_sha256),
        selection.selected_at_ms
    )
}

pub(crate) fn render_registry_entry(
    candidate: &ModelManifestEntry,
    promotion: Option<&PromotionEvidence>,
    artifact_path: &Path,
    promotion_evidence_path: Option<&Path>,
    vision: &RegistryVisionState,
) -> String {
    let evidence_status = if promotion.is_some() {
        "verified-local-promotion"
    } else {
        "source-backed-manifest"
    };
    let evidence_path = promotion_evidence_path
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let backend_version = promotion
        .map(|evidence| evidence.backend_version.as_str())
        .unwrap_or("");
    let benchmark_run_id = promotion
        .map(|evidence| evidence.benchmark_run_id.as_str())
        .unwrap_or("");
    format!(
        "{{\n  \"schemaVersion\": 2,\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"status\": \"installed\",\n  \"evidenceStatus\": \"{}\",\n  \"promotionEvidencePath\": \"{}\",\n  \"backendVersion\": \"{}\",\n  \"benchmarkRunId\": \"{}\",\n  \"upstreamModel\": \"{}\",\n  \"upstreamUrl\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"visionStatus\": \"{}\",\n  \"mmprojPath\": \"{}\",\n  \"mmprojSha256\": \"{}\",\n  \"mmprojSizeBytes\": {},\n  \"licenseSource\": \"{}\",\n  \"licenseCheckedAt\": \"{}\"\n}}\n",
        strict_json::escape_string_content(candidate.id),
        strict_json::escape_string_content(candidate.display_name),
        strict_json::escape_string_content(evidence_status),
        strict_json::escape_string_content(&evidence_path),
        strict_json::escape_string_content(backend_version),
        strict_json::escape_string_content(benchmark_run_id),
        strict_json::escape_string_content(candidate.upstream_model),
        strict_json::escape_string_content(candidate.upstream_url),
        strict_json::escape_string_content(&artifact_path.display().to_string()),
        strict_json::escape_string_content(candidate.sha256.unwrap_or("")),
        strict_json::escape_string_content(&vision.status),
        strict_json::escape_string_content(vision.mmproj_path.as_deref().unwrap_or("")),
        strict_json::escape_string_content(vision.mmproj_sha256.as_deref().unwrap_or("")),
        vision.mmproj_size_bytes.unwrap_or(0),
        strict_json::escape_string_content(candidate.license.source),
        strict_json::escape_string_content(candidate.license.checked_at)
    )
}

pub(crate) fn render_registry_entry_snapshot(entry: &RegistryEntry) -> String {
    format!(
        "{{\n  \"schemaVersion\": 2,\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"status\": \"installed\",\n  \"evidenceStatus\": \"{}\",\n  \"promotionEvidencePath\": \"{}\",\n  \"backendVersion\": \"{}\",\n  \"benchmarkRunId\": \"{}\",\n  \"upstreamModel\": \"{}\",\n  \"upstreamUrl\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"visionStatus\": \"{}\",\n  \"mmprojPath\": \"{}\",\n  \"mmprojSha256\": \"{}\",\n  \"mmprojSizeBytes\": {},\n  \"licenseSource\": \"{}\",\n  \"licenseCheckedAt\": \"{}\"\n}}\n",
        strict_json::escape_string_content(&entry.id),
        strict_json::escape_string_content(&entry.display_name),
        strict_json::escape_string_content(&entry.evidence_status),
        strict_json::escape_string_content(&entry.promotion_evidence_path),
        strict_json::escape_string_content(&entry.backend_version),
        strict_json::escape_string_content(&entry.benchmark_run_id),
        strict_json::escape_string_content(&entry.upstream_model),
        strict_json::escape_string_content(&entry.upstream_url),
        strict_json::escape_string_content(&entry.artifact_path),
        strict_json::escape_string_content(&entry.artifact_sha256),
        strict_json::escape_string_content(&entry.vision_status),
        strict_json::escape_string_content(entry.mmproj_path.as_deref().unwrap_or("")),
        strict_json::escape_string_content(entry.mmproj_sha256.as_deref().unwrap_or("")),
        entry.mmproj_size_bytes.unwrap_or(0),
        strict_json::escape_string_content(&entry.license_source),
        strict_json::escape_string_content(&entry.license_checked_at)
    )
}

pub(crate) fn render_promotion_evidence(
    candidate: &ModelManifestEntry,
    evidence: &PromotionEvidence,
    artifact_path: &Path,
    benchmark: &PromotionBenchmarkEvidence,
    evidence_source: &Path,
) -> String {
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"status\": \"verified-local-promotion\",\n  \"modelId\": \"{}\",\n  \"displayName\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"artifactSizeBytes\": {},\n  \"backendId\": \"{}\",\n  \"backendVersion\": \"{}\",\n  \"backendSmokeEventId\": \"{}\",\n  \"ramFit\": \"{}\",\n  \"recommendedRamGb\": {},\n  \"peakRssBytes\": {},\n  \"mmproj\": \"{}\",\n  \"benchmarkRunId\": \"{}\",\n  \"benchmarkName\": \"{}\",\n  \"benchmarkScore\": {},\n  \"benchmarkLocalPass\": {},\n  \"sourceEvidencePath\": \"{}\",\n  \"recordedAt\": \"{}\"\n}}\n",
        strict_json::escape_string_content(candidate.id),
        strict_json::escape_string_content(candidate.display_name),
        strict_json::escape_string_content(&artifact_path.display().to_string()),
        strict_json::escape_string_content(&evidence.artifact_sha256),
        evidence.artifact_size_bytes,
        strict_json::escape_string_content(&evidence.backend_id),
        strict_json::escape_string_content(&evidence.backend_version),
        strict_json::escape_string_content(&evidence.backend_smoke_event_id),
        strict_json::escape_string_content(&evidence.ram_fit),
        evidence.recommended_ram_gb,
        evidence.peak_rss_bytes,
        strict_json::escape_string_content(&evidence.mmproj),
        strict_json::escape_string_content(&evidence.benchmark_run_id),
        strict_json::escape_string_content(&benchmark.benchmark_name),
        benchmark
            .score
            .map(|score| format!("{score:.6}"))
            .unwrap_or_else(|| "null".to_string()),
        benchmark
            .local_pass
            .map(|value| if value { "true" } else { "false" })
            .unwrap_or("null"),
        strict_json::escape_string_content(&evidence_source.display().to_string()),
        strict_json::escape_string_content(&evidence.recorded_at)
    )
}
