use crate::composition::dispatch;
use crate::foundation::error::AppError;

pub(crate) mod approval_adapter;
pub(crate) mod collaboration_adapter;
mod command_dispatch;
pub(crate) mod context_adapter;
pub(crate) mod evidence_adapter;
pub(crate) mod extensions_adapter;
pub(crate) mod inference_adapter;
pub(crate) mod intent_adapter;
mod monitor_adapter;
pub(crate) mod observability_adapter;
pub(crate) mod ontology_adapter;
pub(crate) mod patch_adapter;
pub(crate) mod policy_adapter;
pub(crate) mod runtime_adapter;
pub(crate) mod tui_adapter;
pub(crate) mod workflow_adapter;

pub fn run(args: impl IntoIterator<Item = String>) -> Result<(), AppError> {
    dispatch::run(args, &mut command_dispatch::CommandDispatchAdapter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_returns_usage_error() {
        let err = run(["wat".to_string()]).unwrap_err();
        assert_eq!(err.code, 2);
        assert!(err.message.contains("알 수 없는 명령"));
    }

    #[test]
    fn unverified_model_install_is_blocked() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-model-install-test-{}", std::process::id()));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let err = run([
            "model".to_string(),
            "install".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert_eq!(err.code, 3);
        assert!(
            err.message.contains("설치를 차단했습니다"),
            "unexpected error: {}",
            err.message
        );
        assert!(err.message.contains("verified 상태로 승격"));
    }

    #[test]
    fn remote_plugin_import_is_rejected() {
        let err = run([
            "plugin".to_string(),
            "import".to_string(),
            "--from".to_string(),
            "codex".to_string(),
            "https://example.com/plugin.git".to_string(),
            "--dry-run".to_string(),
        ])
        .unwrap_err();

        assert_eq!(err.code, 3);
        assert!(err.message.contains("remote URL"));
    }

    #[test]
    fn init_command_reports_layout_without_error() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("rpotato-init-test-{}", std::process::id()));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let result = run(["init".to_string()]);

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn run_requires_active_backend_sidecar() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-run-blocked-test-{}", std::process::id()));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let err = run([
            "run".to_string(),
            "테스트".to_string(),
            "고쳐줘".to_string(),
        ])
        .unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert_eq!(err.code, 3);
        assert!(err.message.contains("backend chat 차단"));
        assert!(err.message.contains("sidecar record"));
    }
}
