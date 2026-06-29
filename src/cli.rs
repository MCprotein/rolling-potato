use crate::app::AppError;

pub const HELP: &str = "\
rpotato

사용법:
  rpotato doctor
  rpotato backend doctor
  rpotato cache status
  rpotato model list
  rpotato model install <id>
  rpotato plugin import --from codex <local-path> --dry-run
  rpotato plugin import --from claude-code <local-path> --dry-run
  rpotato plugin list
  rpotato plugin inspect <id>
  rpotato plugin validate <id>
  rpotato plugin enable <id>
  rpotato plugin disable <id>
  rpotato plugin remove <id> --keep-data
  rpotato plugin remove <id> --purge-data

현재 상태:
  모델과 backend 다운로드는 검증된 manifest가 준비될 때까지 차단됩니다.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Doctor,
    BackendDoctor,
    CacheStatus,
    Model(ModelCommand),
    Plugin(PluginCommand),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ModelCommand {
    List,
    Install { id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum PluginCommand {
    Import {
        source: PluginSource,
        path: String,
        dry_run: bool,
    },
    List,
    Inspect {
        id: String,
    },
    Validate {
        id: String,
    },
    Enable {
        id: String,
    },
    Disable {
        id: String,
    },
    Remove {
        id: String,
        purge_data: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSource {
    Codex,
    ClaudeCode,
}

impl PluginSource {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "codex" => Some(Self::Codex),
            "claude-code" => Some(Self::ClaudeCode),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
        }
    }
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
        [group, action, rest @ ..] if group == "plugin" && action == "import" => {
            parse_plugin_import(rest).map(Command::Plugin)
        }
        [group, action] if group == "plugin" && action == "list" => {
            Ok(Command::Plugin(PluginCommand::List))
        }
        [group, action, id] if group == "plugin" && action == "inspect" => {
            Ok(Command::Plugin(PluginCommand::Inspect { id: id.clone() }))
        }
        [group, action, id] if group == "plugin" && action == "validate" => {
            Ok(Command::Plugin(PluginCommand::Validate { id: id.clone() }))
        }
        [group, action, id] if group == "plugin" && action == "enable" => {
            Ok(Command::Plugin(PluginCommand::Enable { id: id.clone() }))
        }
        [group, action, id] if group == "plugin" && action == "disable" => {
            Ok(Command::Plugin(PluginCommand::Disable { id: id.clone() }))
        }
        [group, action, id, flag] if group == "plugin" && action == "remove" => {
            let purge_data = match flag.as_str() {
                "--keep-data" => false,
                "--purge-data" => true,
                _ => {
                    return Err(AppError::usage(
                        "plugin remove 옵션은 --keep-data 또는 --purge-data만 허용합니다.",
                    ));
                }
            };

            Ok(Command::Plugin(PluginCommand::Remove {
                id: id.clone(),
                purge_data,
            }))
        }
        [group, action, ..] if group == "plugin" && action == "remove" => Err(AppError::usage(
            "plugin id와 삭제 옵션이 필요합니다. 예: rpotato plugin remove imported.example --keep-data",
        )),
        [unknown, ..] => Err(AppError::usage(format!(
            "알 수 없는 명령입니다: {unknown}\n\n{}",
            HELP
        ))),
    }
}

fn parse_plugin_import(args: &[String]) -> Result<PluginCommand, AppError> {
    let mut source = None;
    let mut path = None;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--from" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "plugin import에는 source runtime이 필요합니다. 예: --from codex",
                    ));
                };

                let Some(parsed) = PluginSource::parse(value) else {
                    return Err(AppError::usage(
                        "plugin source는 codex 또는 claude-code만 허용합니다.",
                    ));
                };

                source = Some(parsed);
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(AppError::usage(format!(
                    "알 수 없는 plugin import 옵션입니다: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(AppError::usage(
                        "plugin import local path는 하나만 지정할 수 있습니다.",
                    ));
                }
                path = Some(value.to_string());
                index += 1;
            }
        }
    }

    let Some(source) = source else {
        return Err(AppError::usage(
            "plugin import에는 --from codex 또는 --from claude-code가 필요합니다.",
        ));
    };

    let Some(path) = path else {
        return Err(AppError::usage(
            "plugin import에는 local plugin directory path가 필요합니다.",
        ));
    };

    Ok(PluginCommand::Import {
        source,
        path,
        dry_run,
    })
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

    #[test]
    fn parses_plugin_import_dry_run() {
        let command = parse([
            "plugin".to_string(),
            "import".to_string(),
            "--from".to_string(),
            "codex".to_string(),
            "./my-plugin".to_string(),
            "--dry-run".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Plugin(PluginCommand::Import {
                source: PluginSource::Codex,
                path: "./my-plugin".to_string(),
                dry_run: true
            })
        );
    }
}
