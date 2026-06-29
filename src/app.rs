use crate::backend;
use crate::cache;
use crate::cli::{Command, ModelCommand, MonitorCommand, PluginCommand, UninstallCommand};
use crate::config;
use crate::model;
use crate::monitor;
use crate::plugin;
use crate::runtime;
use crate::state;
use crate::uninstall;

#[derive(Debug, PartialEq, Eq)]
pub struct AppError {
    pub code: u8,
    pub message: String,
}

impl AppError {
    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: message.into(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            code: 2,
            message: message.into(),
        }
    }

    pub fn blocked(message: impl Into<String>) -> Self {
        Self {
            code: 3,
            message: message.into(),
        }
    }
}

pub fn run(args: impl IntoIterator<Item = String>) -> Result<(), AppError> {
    match crate::cli::parse(args)? {
        Command::Help => {
            println!("{}", crate::cli::HELP);
            Ok(())
        }
        Command::Init => {
            println!("{}", runtime::init_report()?);
            Ok(())
        }
        Command::Doctor => {
            println!("{}", runtime::doctor_report());
            Ok(())
        }
        Command::Config => {
            println!("{}", config::report());
            Ok(())
        }
        Command::State => {
            println!("{}", state::status_report()?);
            Ok(())
        }
        Command::Cancel => {
            println!("{}", state::cancel_report()?);
            Ok(())
        }
        Command::BackendDoctor => {
            println!("{}", backend::doctor_report());
            Ok(())
        }
        Command::CacheStatus => {
            println!("{}", cache::status_report());
            Ok(())
        }
        Command::Monitor(MonitorCommand::Status) => {
            println!("{}", monitor::status_report()?);
            Ok(())
        }
        Command::Monitor(MonitorCommand::Models) => {
            println!("{}", monitor::models_report()?);
            Ok(())
        }
        Command::Monitor(MonitorCommand::Export { format }) => {
            print!("{}", monitor::export_report(format)?);
            Ok(())
        }
        Command::Monitor(MonitorCommand::Prune {
            before_days,
            dry_run,
        }) => {
            println!("{}", monitor::prune_report(before_days, dry_run)?);
            Ok(())
        }
        Command::Model(ModelCommand::List) => {
            println!("{}", model::list_report());
            Ok(())
        }
        Command::Model(ModelCommand::Install { id }) => model::install_candidate(&id),
        Command::Plugin(PluginCommand::Import {
            source,
            path,
            dry_run,
        }) => {
            println!("{}", plugin::import_report(source, &path, dry_run)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::List) => {
            println!("{}", plugin::list_report());
            Ok(())
        }
        Command::Plugin(PluginCommand::Inspect { id }) => plugin::not_persisted_yet("inspect", &id),
        Command::Plugin(PluginCommand::Validate { id }) => {
            plugin::not_persisted_yet("validate", &id)
        }
        Command::Plugin(PluginCommand::Enable { id }) => plugin::not_persisted_yet("enable", &id),
        Command::Plugin(PluginCommand::Disable { id }) => plugin::not_persisted_yet("disable", &id),
        Command::Plugin(PluginCommand::Remove { id, purge_data }) => {
            plugin::remove_not_persisted_yet(&id, purge_data)
        }
        Command::Uninstall(UninstallCommand::Plan {
            purge_cache,
            dry_run,
        }) => {
            println!("{}", uninstall::plan_report(purge_cache, dry_run));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn unknown_command_returns_usage_error() {
        let err = run(["wat".to_string()]).unwrap_err();
        assert_eq!(err.code, 2);
        assert!(err.message.contains("알 수 없는 명령"));
    }

    #[test]
    fn unverified_model_install_is_blocked() {
        let err = run([
            "model".to_string(),
            "install".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap_err();
        assert_eq!(err.code, 3);
        assert!(err.message.contains("설치를 차단했습니다"));
        assert!(err.message.contains("GGUF artifact URL"));
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
        let _guard = ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("rpotato-init-test-{}", std::process::id()));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let result = run(["init".to_string()]);

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        assert_eq!(result, Ok(()));
    }
}
