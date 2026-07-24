//! Pure desired-vs-observed backend runtime reconciliation policy.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendRuntimeSpec {
    pub(crate) model_path: PathBuf,
    pub(crate) context_limit_tokens: u32,
    pub(crate) vision_projector_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendRuntimeObservation {
    pub(crate) ready: bool,
    pub(crate) model_path: Option<PathBuf>,
    pub(crate) context_limit_tokens: Option<u32>,
    pub(crate) vision_projector_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendRuntimeDrift {
    NotReady,
    Model,
    Context,
    VisionProjector,
}

pub(crate) fn runtime_drift(
    desired: &BackendRuntimeSpec,
    observed: &BackendRuntimeObservation,
) -> Vec<BackendRuntimeDrift> {
    let mut drift = Vec::new();
    if !observed.ready {
        drift.push(BackendRuntimeDrift::NotReady);
    }
    if observed.model_path.as_ref() != Some(&desired.model_path) {
        drift.push(BackendRuntimeDrift::Model);
    }
    if observed.context_limit_tokens != Some(desired.context_limit_tokens) {
        drift.push(BackendRuntimeDrift::Context);
    }
    if observed.vision_projector_path != desired.vision_projector_path {
        drift.push(BackendRuntimeDrift::VisionProjector);
    }
    drift
}

pub(crate) fn text_runtime_drift(
    desired: &BackendRuntimeSpec,
    observed: &BackendRuntimeObservation,
) -> Vec<BackendRuntimeDrift> {
    runtime_drift(desired, observed)
        .into_iter()
        .filter(|drift| *drift != BackendRuntimeDrift::VisionProjector)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desired() -> BackendRuntimeSpec {
        BackendRuntimeSpec {
            model_path: PathBuf::from("/models/model.gguf"),
            context_limit_tokens: 131_072,
            vision_projector_path: Some(PathBuf::from("/models/mmproj.gguf")),
        }
    }

    #[test]
    fn ready_requires_exact_model_context_and_projector_identity() {
        let desired = desired();
        let aligned = BackendRuntimeObservation {
            ready: true,
            model_path: Some(desired.model_path.clone()),
            context_limit_tokens: Some(131_072),
            vision_projector_path: desired.vision_projector_path.clone(),
        };
        assert!(runtime_drift(&desired, &aligned).is_empty());

        let stale_context = BackendRuntimeObservation {
            context_limit_tokens: Some(4_096),
            ..aligned.clone()
        };
        assert_eq!(
            runtime_drift(&desired, &stale_context),
            [BackendRuntimeDrift::Context]
        );

        let missing_projector = BackendRuntimeObservation {
            vision_projector_path: None,
            ..aligned
        };
        assert_eq!(
            runtime_drift(&desired, &missing_projector),
            [BackendRuntimeDrift::VisionProjector]
        );
    }

    #[test]
    fn model_upgrade_compatibility_text_runtime_ignores_optional_projector_but_not_context() {
        let desired = desired();
        let stale_text_runtime = BackendRuntimeObservation {
            ready: true,
            model_path: Some(desired.model_path.clone()),
            context_limit_tokens: Some(4_096),
            vision_projector_path: None,
        };

        assert_eq!(
            text_runtime_drift(&desired, &stale_text_runtime),
            [BackendRuntimeDrift::Context]
        );

        let aligned_text_runtime = BackendRuntimeObservation {
            context_limit_tokens: Some(131_072),
            ..stale_text_runtime
        };
        assert!(text_runtime_drift(&desired, &aligned_text_runtime).is_empty());
        assert_eq!(
            runtime_drift(&desired, &aligned_text_runtime),
            [BackendRuntimeDrift::VisionProjector]
        );
    }
}
