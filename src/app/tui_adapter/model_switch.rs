//! Transactional TUI model switching with backend and default-selection rollback.

use crate::app::inference_adapter::{backend, model};
use crate::foundation::error::AppError;
use crate::surfaces::tui::setup;

pub(super) trait ModelSwitchPort {
    fn stop_backend(&mut self) -> Result<String, AppError>;
    fn start_backend(&mut self, model_path: &str, context_tokens: u32) -> Result<String, AppError>;
    fn activate_model(&mut self, id: &str) -> Result<(), AppError>;
    fn restore_default_selection(
        &mut self,
        snapshot: &model::DefaultSelectionSnapshot,
    ) -> Result<(), AppError>;
}

pub(super) struct LiveModelSwitch;

impl ModelSwitchPort for LiveModelSwitch {
    fn stop_backend(&mut self) -> Result<String, AppError> {
        backend::stop_report()
    }

    fn start_backend(&mut self, model_path: &str, context_tokens: u32) -> Result<String, AppError> {
        backend::start_report(model_path, Some(context_tokens))
    }

    fn activate_model(&mut self, id: &str) -> Result<(), AppError> {
        model::activate_setup_model(id)
    }

    fn restore_default_selection(
        &mut self,
        snapshot: &model::DefaultSelectionSnapshot,
    ) -> Result<(), AppError> {
        model::restore_default_selection(snapshot)
    }
}

pub(super) fn switch_prepared_model(
    port: &mut impl ModelSwitchPort,
    model_id: &str,
    model_path: &str,
    previous_backend: &backend::BackendRuntimeSnapshot,
    previous_default: &model::DefaultSelectionSnapshot,
) -> Result<String, AppError> {
    if previous_backend.status != "stopped" {
        port.stop_backend()?;
    }
    let started = match port.start_backend(model_path, setup::DEFAULT_CONTEXT_TOKENS) {
        Ok(report) => report,
        Err(error) => {
            return Err(rollback_error(
                port,
                "새 backend 시작",
                error,
                previous_backend,
                previous_default,
            ));
        }
    };
    if let Err(error) = port.activate_model(model_id) {
        return Err(rollback_error(
            port,
            "기본 모델 선택",
            error,
            previous_backend,
            previous_default,
        ));
    }
    Ok(started)
}

fn rollback_error(
    port: &mut impl ModelSwitchPort,
    phase: &str,
    error: AppError,
    previous_backend: &backend::BackendRuntimeSnapshot,
    previous_default: &model::DefaultSelectionSnapshot,
) -> AppError {
    let cleanup = port
        .stop_backend()
        .map(|_| "완료".to_string())
        .unwrap_or_else(|cleanup| format!("실패: {}", cleanup.message));
    let backend_restore = if previous_backend.status == "ready" {
        previous_backend.model_path.as_ref().map_or_else(
            || "실패: 이전 model path 누락".to_string(),
            |path| {
                port.start_backend(
                    &path.display().to_string(),
                    previous_backend
                        .context_limit_tokens
                        .unwrap_or(setup::DEFAULT_CONTEXT_TOKENS),
                )
                .map(|_| "완료".to_string())
                .unwrap_or_else(|restore| format!("실패: {}", restore.message))
            },
        )
    } else {
        "이전 backend가 ready 상태가 아니어서 stopped 유지".to_string()
    };
    let default_restore = port
        .restore_default_selection(previous_default)
        .map(|_| "완료".to_string())
        .unwrap_or_else(|restore| format!("실패: {}", restore.message));
    AppError {
        code: error.code,
        message: format!(
            "{phase} 실패: {}\n- 새 backend 정리: {cleanup}\n- 이전 backend 복구: {backend_restore}\n- 이전 기본 모델 복구: {default_restore}",
            error.message
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingModelSwitch {
        calls: Vec<String>,
    }

    impl ModelSwitchPort for FailingModelSwitch {
        fn stop_backend(&mut self) -> Result<String, AppError> {
            self.calls.push("stop".to_string());
            Ok("stopped".to_string())
        }

        fn start_backend(
            &mut self,
            model_path: &str,
            _context_tokens: u32,
        ) -> Result<String, AppError> {
            self.calls.push(format!("start:{model_path}"));
            if model_path == "/new.gguf" {
                Err(AppError::runtime("injected start failure"))
            } else {
                Ok("ready".to_string())
            }
        }

        fn activate_model(&mut self, id: &str) -> Result<(), AppError> {
            self.calls.push(format!("activate:{id}"));
            Ok(())
        }

        fn restore_default_selection(
            &mut self,
            _snapshot: &model::DefaultSelectionSnapshot,
        ) -> Result<(), AppError> {
            self.calls.push("restore-default".to_string());
            Ok(())
        }
    }

    #[test]
    fn failed_model_start_restores_the_previous_backend_before_returning() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-model-switch-rollback-test-{}",
            std::process::id()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        let default = model::snapshot_default_selection().unwrap();
        let previous = backend::BackendRuntimeSnapshot {
            status: "ready",
            model_id: Some("old".to_string()),
            model_path: Some(std::path::PathBuf::from("/old.gguf")),
            context_limit_tokens: Some(8_192),
            vision_ready: false,
        };
        let mut switch = FailingModelSwitch { calls: Vec::new() };

        let error = switch_prepared_model(&mut switch, "new", "/new.gguf", &previous, &default)
            .unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = std::fs::remove_dir_all(root);
        assert!(error.message.contains("이전 backend 복구: 완료"));
        assert_eq!(
            switch.calls,
            [
                "stop",
                "start:/new.gguf",
                "stop",
                "start:/old.gguf",
                "restore-default"
            ]
        );
    }
}
