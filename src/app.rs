use crate::backend;
use crate::cache;
use crate::cli::{Command, ModelCommand};
use crate::model;

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
}
