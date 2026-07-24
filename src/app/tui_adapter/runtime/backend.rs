//! Lazy backend readiness for interactive TUI requests.

use std::fs;

use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::reconciliation::{
    runtime_drift, BackendRuntimeObservation, BackendRuntimeSpec,
};

pub(super) fn reconcile_existing_runtime() -> Result<(), AppError> {
    let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
    if snapshot.status == "stopped" {
        return Ok(());
    }
    ensure_runtime_ready()
}

pub(super) fn ensure_runtime_ready() -> Result<(), AppError> {
    let configured =
        crate::app::inference_adapter::model::configured_runtime_spec().map_err(|error| {
            if error.message.contains("기본 모델이 선택되지 않았습니다") {
                AppError::blocked(
                    "모델이 선택되지 않았습니다. TUI에서 /model을 입력해 모델을 선택하세요.",
                )
            } else {
                error
            }
        })?;
    let desired = BackendRuntimeSpec {
        model_path: fs::canonicalize(&configured.model_path).map_err(|error| {
            AppError::blocked(format!(
                "기본 모델 artifact를 확인하지 못했습니다.\n- model: {}\n- path: {}\n- 이유: {error}",
                configured.model_id,
                configured.model_path.display()
            ))
        })?,
        context_limit_tokens: configured.context_tokens,
        vision_projector_path: configured.vision_projector_path.clone(),
    };
    let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
    let drift = runtime_drift(
        &desired,
        &BackendRuntimeObservation {
            ready: snapshot.status == "ready",
            model_path: snapshot.model_path.clone(),
            context_limit_tokens: snapshot.context_limit_tokens,
            vision_projector_path: snapshot.vision_projector_path.clone(),
        },
    );
    if drift.is_empty() {
        return Ok(());
    }
    if snapshot.status != "stopped" {
        crate::app::inference_adapter::backend::stop_report()?;
    }
    crate::app::inference_adapter::backend::ensure_installed_report()?;
    crate::app::inference_adapter::backend::start_report(
        &desired.model_path.display().to_string(),
        Some(desired.context_limit_tokens),
    )?;
    let restarted = crate::app::inference_adapter::backend::runtime_snapshot()?;
    let remaining = runtime_drift(
        &desired,
        &BackendRuntimeObservation {
            ready: restarted.status == "ready",
            model_path: restarted.model_path,
            context_limit_tokens: restarted.context_limit_tokens,
            vision_projector_path: restarted.vision_projector_path,
        },
    );
    if remaining.is_empty() {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "backend runtime reconciliation에 실패했습니다.\n- 시작 전 drift: {drift:?}\n- 시작 후 drift: {remaining:?}"
        )))
    }
}
