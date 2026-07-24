//! Lazy backend readiness for interactive TUI requests.

use crate::foundation::error::AppError;

pub(super) fn ensure_runtime_ready() -> Result<(), AppError> {
    let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
    if snapshot.status == "ready" {
        return Ok(());
    }
    if snapshot.status == "stale" {
        crate::app::inference_adapter::backend::stop_report()?;
    }
    let model_path =
        crate::app::inference_adapter::model::default_artifact_path().map_err(|error| {
            if error.message.contains("기본 모델이 선택되지 않았습니다") {
                AppError::blocked(
                    "모델이 선택되지 않았습니다. TUI에서 /model을 입력해 모델을 선택하세요.",
                )
            } else {
                error
            }
        })?;
    crate::app::inference_adapter::backend::ensure_installed_report()?;
    let context_tokens = crate::app::inference_adapter::model::configured_context_length()?;
    crate::app::inference_adapter::backend::start_report(
        &model_path.display().to_string(),
        Some(context_tokens),
    )?;
    Ok(())
}
