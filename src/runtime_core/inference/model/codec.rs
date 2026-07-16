use std::path::Path;

use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::foundation::serialization as strict_json;

use super::manifest::{DefaultSelection, ModelManifestEntry, PromotionEvidence, RegistryEntry};

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
        "{{\n  \"schemaVersion\": 1,\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"status\": \"installed\",\n  \"evidenceStatus\": \"{}\",\n  \"promotionEvidencePath\": \"{}\",\n  \"backendVersion\": \"{}\",\n  \"benchmarkRunId\": \"{}\",\n  \"upstreamModel\": \"{}\",\n  \"upstreamUrl\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"licenseSource\": \"{}\",\n  \"licenseCheckedAt\": \"{}\"\n}}\n",
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
        strict_json::escape_string_content(candidate.license.source),
        strict_json::escape_string_content(candidate.license.checked_at)
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
            "licenseSource",
            "licenseCheckedAt",
        ],
        context,
    )?;
    if strict_json::number(&object, "schemaVersion", context)? != 1 {
        return Err(AppError::blocked("model registry schemaVersion 불일치"));
    }
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
        license_source: strict_json::string(&object, "licenseSource", context)?,
        license_checked_at: strict_json::string(&object, "licenseCheckedAt", context)?,
    })
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
