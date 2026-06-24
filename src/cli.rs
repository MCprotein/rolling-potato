use crate::app::AppError;

pub const HELP: &str = "\
rpotato

사용법:
  rpotato doctor
  rpotato backend doctor
  rpotato cache status
  rpotato model list
  rpotato model install <id>

현재 상태:
  모델과 backend 다운로드는 검증된 manifest가 준비될 때까지 차단됩니다.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Doctor,
    BackendDoctor,
    CacheStatus,
    Model(ModelCommand),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ModelCommand {
    List,
    Install { id: String },
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, AppError> {
    let args: Vec<String> = args.into_iter().collect();

    match args.as_slice() {
        [] => Ok(Command::Help),
        [arg] if arg == "help" || arg == "--help" || arg == "-h" => Ok(Command::Help),
        [arg] if arg == "doctor" => Ok(Command::Doctor),
        [group, action] if group == "backend" && action == "doctor" => Ok(Command::BackendDoctor),
        [group, action] if group == "cache" && action == "status" => Ok(Command::CacheStatus),
        [group, action] if group == "model" && action == "list" => {
            Ok(Command::Model(ModelCommand::List))
        }
        [group, action, id] if group == "model" && action == "install" => {
            Ok(Command::Model(ModelCommand::Install { id: id.clone() }))
        }
        [group, action] if group == "model" && action == "install" => Err(AppError::usage(
            "모델 id가 필요합니다. 예: rpotato model install qwen3.5-4b",
        )),
        [unknown, ..] => Err(AppError::usage(format!(
            "알 수 없는 명령입니다: {unknown}\n\n{}",
            HELP
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_model_install() {
        let command = parse([
            "model".to_string(),
            "install".to_string(),
            "gemma-4-e4b".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Model(ModelCommand::Install {
                id: "gemma-4-e4b".to_string()
            })
        );
    }
}
