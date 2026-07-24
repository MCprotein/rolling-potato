//! Resolves the exact context window for the active inference runtime.

use crate::foundation::error::AppError;

use super::{backend, model};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffectiveContextWindow {
    pub(crate) model_id: String,
    pub(crate) limit_tokens: u32,
}

pub(crate) fn effective_context_window() -> Result<EffectiveContextWindow, AppError> {
    let snapshot = backend::runtime_snapshot()?;
    select_context_window(&snapshot, || {
        let model_id = model::configured_model_id()
            .ok_or_else(|| AppError::blocked("기본 모델이 선택되지 않았습니다."))?;
        let limit_tokens = model::configured_context_length()?;
        Ok(EffectiveContextWindow {
            model_id,
            limit_tokens,
        })
    })
}

fn select_context_window(
    snapshot: &backend::BackendRuntimeSnapshot,
    configured: impl FnOnce() -> Result<EffectiveContextWindow, AppError>,
) -> Result<EffectiveContextWindow, AppError> {
    match (
        snapshot.status,
        snapshot.model_id.as_deref(),
        snapshot.context_limit_tokens.filter(|value| *value > 0),
    ) {
        ("ready", Some(model_id), Some(limit_tokens)) => Ok(EffectiveContextWindow {
            model_id: model_id.to_string(),
            limit_tokens,
        }),
        _ => configured(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn snapshot(
        status: &'static str,
        model_id: Option<&str>,
        limit: Option<u32>,
    ) -> backend::BackendRuntimeSnapshot {
        backend::BackendRuntimeSnapshot {
            status,
            model_id: model_id.map(str::to_string),
            model_path: Some(PathBuf::from("/tmp/model.gguf")),
            context_limit_tokens: limit,
            vision_projector_path: None,
            vision_ready: false,
        }
    }

    #[test]
    fn active_ready_backend_owns_the_effective_context_window() {
        let actual = select_context_window(&snapshot("ready", Some("manual"), Some(1_024)), || {
            panic!("configured fallback must not be read for an active exact runtime")
        })
        .unwrap();

        assert_eq!(
            actual,
            EffectiveContextWindow {
                model_id: "manual".to_string(),
                limit_tokens: 1_024,
            }
        );
    }

    #[test]
    fn incomplete_or_inactive_runtime_uses_the_configured_manifest() {
        let expected = EffectiveContextWindow {
            model_id: "configured".to_string(),
            limit_tokens: 131_072,
        };
        for runtime in [
            snapshot("stopped", None, None),
            snapshot("stale", Some("manual"), Some(1_024)),
            snapshot("ready", Some("manual"), Some(0)),
        ] {
            assert_eq!(
                select_context_window(&runtime, || Ok(expected.clone())).unwrap(),
                expected
            );
        }
    }
}
