use crate::backend;
use crate::cache;
use crate::cli::{Command, ModelCommand, PluginCommand};
use crate::model;
use crate::plugin;

#[derive(Debug, PartialEq, Eq)]
pub struct AppError {
    pub code: u8,
    pub message: String,
}

impl AppError {
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
        Command::Doctor => {
            println!("{}", doctor_report());
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
    }
}

fn doctor_report() -> String {
    let backend = backend::doctor_summary();
    let cache = cache::status_summary();
    let models = model::candidate_summary();

    format!(
        "rpotato 진단\n- CLI: 사용 가능\n- backend: {}\n- model: {}\n- cache: {}",
        backend, models, cache
    )
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
}
