use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;

use super::super::manifest::PromotionEvidence;

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
