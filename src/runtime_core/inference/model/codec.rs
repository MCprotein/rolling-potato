use std::path::Path;

use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::foundation::serialization as strict_json;

use super::manifest::{
    DefaultSelection, ModelManifestEntry, PromotionEvidence, RegistryEntry, RegistryVisionState,
};
use super::promotion::PromotionBenchmarkEvidence;

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

pub(crate) fn parse_registry_entry(text: &str) -> Result<RegistryEntry, AppError> {
    let context = "model registry entry";
    let object = strict_json::parse_object(
        text,
        &[
            "schemaVersion",
            "id",
            "displayName",
            "status",
            "evidenceStatus",
            "promotionEvidencePath",
            "backendVersion",
            "benchmarkRunId",
            "upstreamModel",
            "upstreamUrl",
            "artifactPath",
            "artifactSha256",
            "visionStatus",
            "mmprojPath",
            "mmprojSha256",
            "mmprojSizeBytes",
            "licenseSource",
            "licenseCheckedAt",
        ],
        context,
    )?;
    let schema_version = strict_json::number(&object, "schemaVersion", context)?;
    if !matches!(schema_version, 1 | 2) {
        return Err(AppError::blocked("model registry schemaVersion 불일치"));
    }
    let vision = if schema_version == 1 {
        RegistryVisionState {
            status: "unavailable-legacy".to_string(),
            mmproj_path: None,
            mmproj_sha256: None,
            mmproj_size_bytes: None,
        }
    } else {
        parse_registry_vision(&object, context)?
    };
    Ok(RegistryEntry {
        id: strict_json::string(&object, "id", context)?,
        display_name: strict_json::string(&object, "displayName", context)?,
        status: strict_json::string(&object, "status", context)?,
        evidence_status: strict_json::string(&object, "evidenceStatus", context)?,
        promotion_evidence_path: strict_json::string(&object, "promotionEvidencePath", context)?,
        backend_version: strict_json::string(&object, "backendVersion", context)?,
        benchmark_run_id: strict_json::string(&object, "benchmarkRunId", context)?,
        upstream_model: strict_json::string(&object, "upstreamModel", context)?,
        upstream_url: strict_json::string(&object, "upstreamUrl", context)?,
        artifact_path: strict_json::string(&object, "artifactPath", context)?,
        artifact_sha256: strict_json::string(&object, "artifactSha256", context)?,
        vision_status: vision.status,
        mmproj_path: vision.mmproj_path,
        mmproj_sha256: vision.mmproj_sha256,
        mmproj_size_bytes: vision.mmproj_size_bytes,
        license_source: strict_json::string(&object, "licenseSource", context)?,
        license_checked_at: strict_json::string(&object, "licenseCheckedAt", context)?,
    })
}

fn parse_registry_vision(
    object: &strict_json::Object,
    context: &str,
) -> Result<RegistryVisionState, AppError> {
    let status = strict_json::string(object, "visionStatus", context)?;
    let path = strict_json::string(object, "mmprojPath", context)?;
    let sha256 = strict_json::string(object, "mmprojSha256", context)?;
    let size_bytes = strict_json::number(object, "mmprojSizeBytes", context)?;
    match status.as_str() {
        "ready" => {
            if path.trim().is_empty() || !checksum::is_valid_sha256(&sha256) || size_bytes == 0 {
                return Err(AppError::blocked(
                    "vision-ready model registry에는 유효한 mmproj path, SHA-256, size가 필요합니다.",
                ));
            }
            Ok(RegistryVisionState {
                status,
                mmproj_path: Some(path),
                mmproj_sha256: Some(sha256),
                mmproj_size_bytes: Some(size_bytes),
            })
        }
        "unavailable" => {
            if !path.is_empty() || !sha256.is_empty() || size_bytes != 0 {
                return Err(AppError::blocked(
                    "vision unavailable model registry에는 mmproj artifact를 기록할 수 없습니다.",
                ));
            }
            Ok(RegistryVisionState {
                status,
                mmproj_path: None,
                mmproj_sha256: None,
                mmproj_size_bytes: None,
            })
        }
        _ => Err(AppError::blocked(
            "model registry visionStatus는 ready 또는 unavailable이어야 합니다.",
        )),
    }
}

pub(crate) fn parse_default_selection(text: &str) -> Result<DefaultSelection, AppError> {
    let context = "default model selection";
    let object = strict_json::parse_object(
        text,
        &["schemaVersion", "modelId", "artifactSha256", "selectedAtMs"],
        context,
    )?;
    if strict_json::number(&object, "schemaVersion", context)? != 1 {
        return Err(AppError::blocked("default model schemaVersion 불일치"));
    }
    Ok(DefaultSelection {
        model_id: strict_json::string(&object, "modelId", context)?,
        artifact_sha256: strict_json::string(&object, "artifactSha256", context)?,
        selected_at_ms: strict_json::number(&object, "selectedAtMs", context)?,
    })
}

pub(crate) fn parse_promotion_evidence(text: &str) -> Result<PromotionEvidence, AppError> {
    let schema_version = required_json_u64(text, "schemaVersion")?;
    if schema_version != 1 {
        return Err(AppError::usage(format!(
            "model promotion evidence schemaVersion은 1이어야 합니다: {schema_version}"
        )));
    }

    let artifact_sha256 = required_json_string(text, "artifactSha256")?;
    if !checksum::is_valid_sha256(&artifact_sha256) {
        return Err(AppError::usage(
            "model promotion evidence artifactSha256은 64자리 hex string이어야 합니다.",
        ));
    }

    Ok(PromotionEvidence {
        model_id: required_json_string(text, "modelId")?,
        artifact_sha256,
        artifact_size_bytes: required_json_u64(text, "artifactSizeBytes")?,
        backend_id: required_json_string(text, "backendId")?,
        backend_version: required_json_string(text, "backendVersion")?,
        backend_smoke_event_id: required_json_string(text, "backendSmokeEventId")?,
        ram_fit: required_json_string(text, "ramFit")?,
        recommended_ram_gb: required_json_u32(text, "recommendedRamGb")?,
        peak_rss_bytes: required_json_u64(text, "peakRssBytes")?,
        mmproj: required_json_string(text, "mmproj")?,
        benchmark_run_id: required_json_string(text, "benchmarkRunId")?,
        recorded_at: required_json_string(text, "recordedAt")?,
    })
}

fn required_json_string(text: &str, key: &str) -> Result<String, AppError> {
    extract_json_string(text, key).ok_or_else(|| {
        AppError::usage(format!(
            "model promotion evidence에 필수 string field가 없습니다: {key}"
        ))
    })
}

fn required_json_u64(text: &str, key: &str) -> Result<u64, AppError> {
    extract_json_u64(text, key).ok_or_else(|| {
        AppError::usage(format!(
            "model promotion evidence에 필수 number field가 없습니다: {key}"
        ))
    })
}

fn required_json_u32(text: &str, key: &str) -> Result<u32, AppError> {
    let value = required_json_u64(text, key)?;
    u32::try_from(value).map_err(|_| {
        AppError::usage(format!(
            "model promotion evidence number field가 u32 범위를 넘습니다: {key}"
        ))
    })
}

fn extract_json_string(text: &str, key: &str) -> Option<String> {
    let raw_value = json_value_after_key(text, key)?.strip_prefix('"')?;
    let mut parsed = String::new();
    let mut escaped = false;

    for ch in raw_value.chars() {
        if escaped {
            match ch {
                '"' => parsed.push('"'),
                '\\' => parsed.push('\\'),
                'n' => parsed.push('\n'),
                'r' => parsed.push('\r'),
                't' => parsed.push('\t'),
                other => parsed.push(other),
            }
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(parsed),
            other => parsed.push(other),
        }
    }

    None
}

fn extract_json_u64(text: &str, key: &str) -> Option<u64> {
    let value = json_value_after_key(text, key)?;
    let digits = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }

    digits.parse().ok()
}

fn json_value_after_key<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let quoted_key = format!("\"{key}\"");
    let key_start = text.find(&quoted_key)?;
    let after_key = &text[key_start + quoted_key.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
