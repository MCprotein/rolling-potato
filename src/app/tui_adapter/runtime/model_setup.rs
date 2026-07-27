//! Prepared model activation and user-facing cache/download reporting.

use super::super::model_switch::{switch_prepared_model, LiveModelSwitch};
use crate::foundation::error::AppError;

pub(super) fn setup(id: &str) -> Result<String, AppError> {
    crate::app::inference_adapter::backend::ensure_installed_report()?;
    let prepared = crate::app::inference_adapter::model::prepare_setup_model(id)?;
    let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
    let default = crate::app::inference_adapter::model::snapshot_default_selection()?;
    switch_prepared_model(
        &mut LiveModelSwitch,
        &prepared.id,
        &prepared.artifact_path.display().to_string(),
        prepared.context_tokens,
        &snapshot,
        &default,
    )?;
    Ok(format!(
        "모델 변경 완료\n- model: {}\n- model artifact: {}\n- context: {}\n- vision: {}\n- backend: ready",
        prepared.id,
        match prepared.artifact_fetch_status {
            crate::runtime_core::inference::model::manifest::ModelArtifactFetchStatus::CacheHit =>
                "기존 cache 재사용",
            crate::runtime_core::inference::model::manifest::ModelArtifactFetchStatus::Resumed =>
                "partial download 이어받기 완료",
            crate::runtime_core::inference::model::manifest::ModelArtifactFetchStatus::Downloaded =>
                "download 및 SHA-256 검증 완료",
        },
        prepared.context_tokens,
        prepared.vision.as_str(),
    ))
}
