//! Shared backend formatting, time, identity, and vision-readiness helpers.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;
use crate::runtime_core::inference::model::manifest::{
    model_id_for_artifact_hash, source_backed_vision_projector, CANDIDATES,
};

pub(super) const HEALTH_TIMEOUT_MS: u64 = 500;
pub(super) const TERMINAL_RECORD_RETENTION_MS: u128 = 5 * 60 * 1_000;

pub(super) fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "model-default".to_string())
}

pub(super) fn display_optional_u128(value: Option<u128>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn model_identity(record: &BackendSidecarRecord) -> String {
    model_id_for_artifact_hash(&record.model_sha256)
        .map(str::to_string)
        .unwrap_or_else(|| format!("unregistered-artifact:{}", record.model_sha256))
}

pub(super) fn vision_readiness(record: &BackendSidecarRecord) -> &'static str {
    let Some(candidate) = CANDIDATES
        .iter()
        .find(|candidate| candidate.sha256 == Some(record.model_sha256.as_str()))
    else {
        return "unavailable (text-ready)";
    };
    if source_backed_vision_projector(candidate).is_none() {
        return "unsupported (text-ready)";
    }
    supported_vision_readiness(runtime_vision_projector_ready(record))
}

pub(super) fn supported_vision_readiness(projector_ready: bool) -> &'static str {
    if projector_ready {
        "ready"
    } else {
        "on-demand (text-ready)"
    }
}

pub(super) fn runtime_vision_projector_ready(record: &BackendSidecarRecord) -> bool {
    let Some(verified) = crate::app::inference_adapter::model::verified_vision_projector(
        &record.model_path,
        &record.model_sha256,
    ) else {
        return false;
    };
    runtime_binding_matches(
        record,
        &verified.path,
        &verified.sha256,
        verified.size_bytes,
    )
}

pub(super) fn runtime_binding_matches(
    record: &BackendSidecarRecord,
    verified_path: &Path,
    verified_sha256: &str,
    verified_size_bytes: u64,
) -> bool {
    record.mmproj_path.as_deref() == Some(verified_path)
        && record.mmproj_sha256.as_deref() == Some(verified_sha256)
        && record.mmproj_size_bytes == Some(verified_size_bytes)
}

pub(super) fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
