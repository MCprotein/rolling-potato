use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::foundation::serialization as strict_json;

use super::super::manifest::{RegistryEntry, RegistryVisionState};

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
