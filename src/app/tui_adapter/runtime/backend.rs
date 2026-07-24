//! Lazy backend readiness for interactive TUI requests.

use std::fs;

use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::reconciliation::{
    runtime_drift, text_runtime_drift, BackendRuntimeDrift, BackendRuntimeObservation,
    BackendRuntimeSpec,
};
use crate::surfaces::tui::runtime_bridge::TuiVisionStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeRequirement {
    Text,
    Vision,
}

pub(super) fn reconcile_existing_runtime() -> Result<(), AppError> {
    let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
    if snapshot.status == "stopped" {
        return Ok(());
    }
    ensure_runtime_ready(RuntimeRequirement::Text)
}

pub(super) fn ensure_runtime_ready(requirement: RuntimeRequirement) -> Result<(), AppError> {
    let configured = match requirement {
        RuntimeRequirement::Text => crate::app::inference_adapter::model::configured_runtime_spec(),
        RuntimeRequirement::Vision => {
            crate::app::inference_adapter::model::configured_vision_runtime_spec()
        }
    }
    .map_err(|error| {
        if error.message.contains("기본 모델이 선택되지 않았습니다") {
            AppError::blocked(
                "모델이 선택되지 않았습니다. TUI에서 /model을 입력해 모델을 선택하세요.",
            )
        } else {
            error
        }
    })?;
    let desired = desired_runtime_spec(&configured)?;
    let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
    let drift = requirement_drift(
        requirement,
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
    let remaining = requirement_drift(
        requirement,
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

pub(super) fn vision_status(
    snapshot: Option<&crate::app::inference_adapter::backend::BackendRuntimeSnapshot>,
) -> TuiVisionStatus {
    let Ok(configured) = crate::app::inference_adapter::model::configured_vision_runtime() else {
        return TuiVisionStatus::Unavailable;
    };
    let desired = configured
        .runtime
        .vision_projector_path
        .as_ref()
        .and_then(|_| desired_runtime_spec(&configured.runtime).ok());
    classify_vision_status(Some(&configured), desired.as_ref(), snapshot)
}

fn classify_vision_status(
    configured: Option<&crate::app::inference_adapter::model::ConfiguredVisionRuntime>,
    desired: Option<&BackendRuntimeSpec>,
    snapshot: Option<&crate::app::inference_adapter::backend::BackendRuntimeSnapshot>,
) -> TuiVisionStatus {
    let Some(configured) = configured else {
        return TuiVisionStatus::Unavailable;
    };
    if !configured.projector_supported {
        return TuiVisionStatus::Unsupported;
    }
    let (Some(desired), Some(snapshot)) = (desired, snapshot) else {
        return TuiVisionStatus::OnDemand;
    };
    let observed = BackendRuntimeObservation {
        ready: snapshot.status == "ready",
        model_path: snapshot.model_path.clone(),
        context_limit_tokens: snapshot.context_limit_tokens,
        vision_projector_path: snapshot.vision_projector_path.clone(),
    };
    if runtime_drift(desired, &observed).is_empty() {
        TuiVisionStatus::Ready
    } else {
        TuiVisionStatus::OnDemand
    }
}

fn desired_runtime_spec(
    configured: &crate::app::inference_adapter::model::ConfiguredRuntimeSpec,
) -> Result<BackendRuntimeSpec, AppError> {
    Ok(BackendRuntimeSpec {
        model_path: fs::canonicalize(&configured.model_path).map_err(|error| {
            AppError::blocked(format!(
                "기본 모델 artifact를 확인하지 못했습니다.\n- model: {}\n- path: {}\n- 이유: {error}",
                configured.model_id,
                configured.model_path.display()
            ))
        })?,
        context_limit_tokens: configured.context_tokens,
        vision_projector_path: configured.vision_projector_path.clone(),
    })
}

fn requirement_drift(
    requirement: RuntimeRequirement,
    desired: &BackendRuntimeSpec,
    observed: &BackendRuntimeObservation,
) -> Vec<BackendRuntimeDrift> {
    match requirement {
        RuntimeRequirement::Text => text_runtime_drift(desired, observed),
        RuntimeRequirement::Vision => runtime_drift(desired, observed),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::app::inference_adapter::backend::BackendRuntimeSnapshot;
    use crate::app::inference_adapter::model::{ConfiguredRuntimeSpec, ConfiguredVisionRuntime};

    fn configured(
        model_id: &str,
        projector_supported: bool,
        projector_ready: bool,
    ) -> ConfiguredVisionRuntime {
        ConfiguredVisionRuntime {
            runtime: ConfiguredRuntimeSpec {
                model_id: model_id.to_string(),
                model_path: PathBuf::from("/models/qwen.gguf"),
                context_tokens: 262_144,
                vision_projector_path: projector_ready
                    .then(|| PathBuf::from("/models/mmproj.gguf")),
            },
            projector_supported,
        }
    }

    fn desired() -> BackendRuntimeSpec {
        BackendRuntimeSpec {
            model_path: PathBuf::from("/models/qwen.gguf"),
            context_limit_tokens: 262_144,
            vision_projector_path: Some(PathBuf::from("/models/mmproj.gguf")),
        }
    }

    fn snapshot(status: &'static str) -> BackendRuntimeSnapshot {
        BackendRuntimeSnapshot {
            status,
            model_id: Some("qwen3.5-4b".to_string()),
            model_path: Some(PathBuf::from("/models/qwen.gguf")),
            context_limit_tokens: Some(262_144),
            vision_projector_path: Some(PathBuf::from("/models/mmproj.gguf")),
        }
    }

    #[test]
    fn model_upgrade_compatibility_tui_vision_status_requires_exact_runtime_identity() {
        let configured = configured("qwen3.5-4b", true, true);
        let desired = desired();
        assert_eq!(
            classify_vision_status(Some(&configured), Some(&desired), Some(&snapshot("ready"))),
            TuiVisionStatus::Ready
        );
        assert_eq!(
            classify_vision_status(Some(&configured), Some(&desired), Some(&snapshot("stale"))),
            TuiVisionStatus::OnDemand
        );

        let mut wrong_context = snapshot("ready");
        wrong_context.context_limit_tokens = Some(4_096);
        assert_eq!(
            classify_vision_status(Some(&configured), Some(&desired), Some(&wrong_context)),
            TuiVisionStatus::OnDemand
        );

        let mut wrong_projector = snapshot("ready");
        wrong_projector.vision_projector_path = Some(PathBuf::from("/models/other-mmproj.gguf"));
        assert_eq!(
            classify_vision_status(Some(&configured), Some(&desired), Some(&wrong_projector)),
            TuiVisionStatus::OnDemand
        );
    }

    #[test]
    fn model_upgrade_compatibility_tui_vision_status_distinguishes_capability_from_readiness() {
        for model_id in ["qwen3.5-4b", "gemma-4-e4b"] {
            let candidate =
                crate::runtime_core::inference::model::manifest::find_candidate(model_id).unwrap();
            let supported = configured(
                model_id,
                crate::runtime_core::inference::model::manifest::source_backed_vision_projector(
                    candidate,
                )
                .is_some(),
                false,
            );
            assert_eq!(
                classify_vision_status(Some(&supported), None, Some(&snapshot("stopped"))),
                TuiVisionStatus::OnDemand,
                "{model_id}"
            );
        }
        assert_eq!(
            classify_vision_status(Some(&configured("text-only", false, false)), None, None),
            TuiVisionStatus::Unsupported
        );
        assert_eq!(
            classify_vision_status(None, None, None),
            TuiVisionStatus::Unavailable
        );
    }
}
