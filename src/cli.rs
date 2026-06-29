use crate::app::AppError;

pub const HELP: &str = "\
rpotato

사용법:
  rpotato doctor
  rpotato init
  rpotato run \"<request>\"
  rpotato intent classify \"<request>\"
  rpotato intent routes
  rpotato config
  rpotato state
  rpotato state reconcile
  rpotato state resume
  rpotato cancel
  rpotato evidence validate <artifact-pointer>
  rpotato skill list
  rpotato skill run <id>
  rpotato policy schema
  rpotato policy check-command <command>
  rpotato policy check-path --read <path>
  rpotato policy check-path --write <path>
  rpotato policy redact <text>
  rpotato hooks list
  rpotato hooks validate-result <json>
  rpotato backend doctor
  rpotato backend install-plan
  rpotato backend verify-archive <path> --sha256 <hash>
  rpotato cache status
  rpotato monitor status
  rpotato monitor models
  rpotato monitor export --format jsonl
  rpotato monitor export --format csv
  rpotato monitor prune --before 30d --dry-run
  rpotato model list
  rpotato model manifest
  rpotato model inspect <id>
  rpotato model registry
  rpotato model download-plan <id>
  rpotato model verify-file <path> --sha256 <hash>
  rpotato model cleanup-failed <id> --dry-run
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
  rpotato uninstall --keep-cache
  rpotato uninstall --purge-cache
  rpotato uninstall --dry-run --purge-cache

현재 상태:
  모델과 backend 다운로드는 검증된 manifest가 준비될 때까지 차단됩니다.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Init,
    Run { request: String },
    Intent(IntentCommand),
    Doctor,
    Config,
    State(StateCommand),
    Cancel,
    Evidence(EvidenceCommand),
    Skill(SkillCommand),
    Policy(PolicyCommand),
    Hooks(HooksCommand),
    Backend(BackendCommand),
    CacheStatus,
    Monitor(MonitorCommand),
    Model(ModelCommand),
    Plugin(PluginCommand),
    Uninstall(UninstallCommand),
}

#[derive(Debug, PartialEq, Eq)]
pub enum MonitorCommand {
    Status,
    Models,
    Export { format: MonitorExportFormat },
    Prune { before_days: u64, dry_run: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub enum StateCommand {
    Status,
    Reconcile,
    Resume,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EvidenceCommand {
    Validate { pointer: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum SkillCommand {
    List,
    Run { id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum IntentCommand {
    Classify { request: String },
    Routes,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PolicyCommand {
    Schema,
    CheckCommand { command: String },
    CheckPath { mode: PolicyPathMode, path: String },
    Redact { text: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyPathMode {
    Read,
    Write,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HooksCommand {
    List,
    ValidateResult { json: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BackendCommand {
    Doctor,
    InstallPlan,
    VerifyArchive { path: String, sha256: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum MonitorExportFormat {
    Jsonl,
    Csv,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ModelCommand {
    List,
    Manifest,
    Inspect { id: String },
    Registry,
    DownloadPlan { id: String },
    VerifyFile { path: String, sha256: String },
    CleanupFailed { id: String, dry_run: bool },
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

#[derive(Debug, PartialEq, Eq)]
pub enum UninstallCommand {
    Plan { purge_cache: bool, dry_run: bool },
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
        [arg] if arg == "init" => Ok(Command::Init),
        [group, rest @ ..] if group == "run" => Ok(Command::Run {
            request: parse_request(rest, "run")?,
        }),
        [group, action, rest @ ..] if group == "intent" && action == "classify" => {
            Ok(Command::Intent(IntentCommand::Classify {
                request: parse_request(rest, "intent classify")?,
            }))
        }
        [group, action] if group == "intent" && action == "routes" => {
            Ok(Command::Intent(IntentCommand::Routes))
        }
        [group, ..] if group == "intent" => {
            Err(AppError::usage("intent 명령은 classify만 허용합니다."))
        }
        [arg] if arg == "doctor" => Ok(Command::Doctor),
        [arg] if arg == "config" => Ok(Command::Config),
        [arg] if arg == "state" => Ok(Command::State(StateCommand::Status)),
        [group, action] if group == "state" && action == "reconcile" => {
            Ok(Command::State(StateCommand::Reconcile))
        }
        [group, action] if group == "state" && action == "resume" => {
            Ok(Command::State(StateCommand::Resume))
        }
        [group, ..] if group == "state" => Err(AppError::usage(
            "state 명령은 status 생략형, reconcile, resume만 허용합니다.",
        )),
        [arg] if arg == "cancel" => Ok(Command::Cancel),
        [group, action, pointer] if group == "evidence" && action == "validate" => {
            Ok(Command::Evidence(EvidenceCommand::Validate {
                pointer: pointer.clone(),
            }))
        }
        [group, action, ..] if group == "evidence" && action == "validate" => Err(
            AppError::usage("evidence validate에는 artifact pointer가 필요합니다."),
        ),
        [group, ..] if group == "evidence" => {
            Err(AppError::usage("evidence 명령은 validate만 허용합니다."))
        }
        [group, action] if group == "skill" && action == "list" => {
            Ok(Command::Skill(SkillCommand::List))
        }
        [group, action, id] if group == "skill" && action == "run" => {
            Ok(Command::Skill(SkillCommand::Run { id: id.clone() }))
        }
        [group, action, ..] if group == "skill" && action == "run" => Err(AppError::usage(
            "skill run에는 skill id가 필요합니다. 예: rpotato skill run fix-test",
        )),
        [group, ..] if group == "skill" => {
            Err(AppError::usage("skill 명령은 list 또는 run만 허용합니다."))
        }
        [group, action] if group == "policy" && action == "schema" => {
            Ok(Command::Policy(PolicyCommand::Schema))
        }
        [group, action, rest @ ..] if group == "policy" && action == "check-command" => {
            Ok(Command::Policy(PolicyCommand::CheckCommand {
                command: parse_request(rest, "policy check-command")?,
            }))
        }
        [group, action, flag, path] if group == "policy" && action == "check-path" => {
            let mode = match flag.as_str() {
                "--read" => PolicyPathMode::Read,
                "--write" => PolicyPathMode::Write,
                _ => {
                    return Err(AppError::usage(
                        "policy check-path는 --read 또는 --write만 허용합니다.",
                    ));
                }
            };
            Ok(Command::Policy(PolicyCommand::CheckPath {
                mode,
                path: path.clone(),
            }))
        }
        [group, action, rest @ ..] if group == "policy" && action == "redact" => {
            Ok(Command::Policy(PolicyCommand::Redact {
                text: parse_request(rest, "policy redact")?,
            }))
        }
        [group, ..] if group == "policy" => Err(AppError::usage(
            "policy 명령은 schema, check-command, check-path, redact만 허용합니다.",
        )),
        [group, action] if group == "hooks" && action == "list" => {
            Ok(Command::Hooks(HooksCommand::List))
        }
        [group, action, rest @ ..] if group == "hooks" && action == "validate-result" => {
            Ok(Command::Hooks(HooksCommand::ValidateResult {
                json: parse_request(rest, "hooks validate-result")?,
            }))
        }
        [group, ..] if group == "hooks" => Err(AppError::usage(
            "hooks 명령은 list 또는 validate-result만 허용합니다.",
        )),
        [group, action] if group == "backend" && action == "doctor" => {
            Ok(Command::Backend(BackendCommand::Doctor))
        }
        [group, action] if group == "backend" && action == "install-plan" => {
            Ok(Command::Backend(BackendCommand::InstallPlan))
        }
        [group, action, path, flag, sha256]
            if group == "backend" && action == "verify-archive" && flag == "--sha256" =>
        {
            Ok(Command::Backend(BackendCommand::VerifyArchive {
                path: path.clone(),
                sha256: sha256.clone(),
            }))
        }
        [group, action, ..] if group == "backend" && action == "verify-archive" => Err(
            AppError::usage("backend verify-archive는 <path> --sha256 <hash> 형식이 필요합니다."),
        ),
        [group, ..] if group == "backend" => Err(AppError::usage(
            "backend 명령은 doctor, install-plan, verify-archive만 허용합니다.",
        )),
        [group, action] if group == "cache" && action == "status" => Ok(Command::CacheStatus),
        [group, action] if group == "monitor" && action == "status" => {
            Ok(Command::Monitor(MonitorCommand::Status))
        }
        [group, action] if group == "monitor" && action == "models" => {
            Ok(Command::Monitor(MonitorCommand::Models))
        }
        [group, action, rest @ ..] if group == "monitor" && action == "export" => {
            parse_monitor_export(rest).map(Command::Monitor)
        }
        [group, action, rest @ ..] if group == "monitor" && action == "prune" => {
            parse_monitor_prune(rest).map(Command::Monitor)
        }
        [group, ..] if group == "monitor" => Err(AppError::usage(
            "monitor 명령은 status, models, export, prune만 허용합니다.",
        )),
        [group, action] if group == "model" && action == "list" => {
            Ok(Command::Model(ModelCommand::List))
        }
        [group, action] if group == "model" && action == "manifest" => {
            Ok(Command::Model(ModelCommand::Manifest))
        }
        [group, action, id] if group == "model" && action == "inspect" => {
            Ok(Command::Model(ModelCommand::Inspect { id: id.clone() }))
        }
        [group, action] if group == "model" && action == "registry" => {
            Ok(Command::Model(ModelCommand::Registry))
        }
        [group, action, id] if group == "model" && action == "download-plan" => {
            Ok(Command::Model(ModelCommand::DownloadPlan { id: id.clone() }))
        }
        [group, action, path, flag, sha256]
            if group == "model" && action == "verify-file" && flag == "--sha256" =>
        {
            Ok(Command::Model(ModelCommand::VerifyFile {
                path: path.clone(),
                sha256: sha256.clone(),
            }))
        }
        [group, action, ..] if group == "model" && action == "verify-file" => Err(
            AppError::usage("model verify-file은 <path> --sha256 <hash> 형식이 필요합니다."),
        ),
        [group, action, id, flag] if group == "model" && action == "cleanup-failed" => {
            let dry_run = match flag.as_str() {
                "--dry-run" => true,
                "--delete" => false,
                _ => {
                    return Err(AppError::usage(
                        "model cleanup-failed는 --dry-run 또는 --delete가 필요합니다.",
                    ));
                }
            };
            Ok(Command::Model(ModelCommand::CleanupFailed {
                id: id.clone(),
                dry_run,
            }))
        }
        [group, action, ..] if group == "model" && action == "cleanup-failed" => {
            Err(AppError::usage(
                "model cleanup-failed는 <id> --dry-run 또는 <id> --delete 형식이 필요합니다.",
            ))
        }
        [group, action, id] if group == "model" && action == "install" => {
            Ok(Command::Model(ModelCommand::Install { id: id.clone() }))
        }
        [group, action] if group == "model" && action == "install" => Err(AppError::usage(
            "모델 id가 필요합니다. 예: rpotato model install qwen3.5-4b",
        )),
        [group, ..] if group == "model" => Err(AppError::usage(
            "model 명령은 list, manifest, inspect, registry, download-plan, verify-file, cleanup-failed, install만 허용합니다.",
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
        [group, rest @ ..] if group == "uninstall" => {
            parse_uninstall(rest).map(Command::Uninstall)
        }
        [unknown, ..] => Err(AppError::usage(format!(
            "알 수 없는 명령입니다: {unknown}\n\n{}",
            HELP
        ))),
    }
}

fn parse_uninstall(args: &[String]) -> Result<UninstallCommand, AppError> {
    let mut keep_cache = false;
    let mut purge_cache = false;
    let mut dry_run = false;

    for arg in args {
        match arg.as_str() {
            "--keep-cache" => keep_cache = true,
            "--purge-cache" => purge_cache = true,
            "--dry-run" => dry_run = true,
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 uninstall 옵션입니다: {unknown}"
                )));
            }
        }
    }

    if keep_cache == purge_cache {
        return Err(AppError::usage(
            "uninstall은 --keep-cache 또는 --purge-cache 중 하나가 필요합니다.",
        ));
    }

    Ok(UninstallCommand::Plan {
        purge_cache,
        dry_run,
    })
}

fn parse_request(args: &[String], command: &str) -> Result<String, AppError> {
    if args.is_empty() {
        return Err(AppError::usage(format!(
            "{command}에는 request 문자열이 필요합니다."
        )));
    }

    let request = args.join(" ");
    if request.trim().is_empty() {
        return Err(AppError::usage(format!(
            "{command}에는 비어 있지 않은 request가 필요합니다."
        )));
    }

    Ok(request)
}

fn parse_monitor_export(args: &[String]) -> Result<MonitorCommand, AppError> {
    match args {
        [flag, format] if flag == "--format" => {
            let format = match format.as_str() {
                "jsonl" => MonitorExportFormat::Jsonl,
                "csv" => MonitorExportFormat::Csv,
                _ => {
                    return Err(AppError::usage(
                        "monitor export format은 jsonl 또는 csv만 허용합니다.",
                    ));
                }
            };
            Ok(MonitorCommand::Export { format })
        }
        _ => Err(AppError::usage(
            "monitor export에는 --format jsonl 또는 --format csv가 필요합니다.",
        )),
    }
}

fn parse_monitor_prune(args: &[String]) -> Result<MonitorCommand, AppError> {
    let mut before_days = None;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--before" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "monitor prune에는 --before 30d 같은 기간이 필요합니다.",
                    ));
                };
                before_days = Some(parse_days(value)?);
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 monitor prune 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let Some(before_days) = before_days else {
        return Err(AppError::usage(
            "monitor prune에는 --before 30d 같은 기간이 필요합니다.",
        ));
    };

    if !dry_run {
        return Err(AppError::usage(
            "monitor prune은 현재 --dry-run만 허용합니다.",
        ));
    }

    Ok(MonitorCommand::Prune {
        before_days,
        dry_run,
    })
}

fn parse_days(value: &str) -> Result<u64, AppError> {
    let Some(days) = value.strip_suffix('d') else {
        return Err(AppError::usage(
            "기간은 day 단위만 허용합니다. 예: --before 30d",
        ));
    };

    let parsed = days
        .parse::<u64>()
        .map_err(|_| AppError::usage("기간은 양의 정수 day 단위여야 합니다. 예: --before 30d"))?;

    if parsed == 0 {
        return Err(AppError::usage("기간은 1d 이상이어야 합니다."));
    }

    Ok(parsed)
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
    fn parses_model_manifest() {
        let command = parse(["model".to_string(), "manifest".to_string()]).unwrap();
        assert_eq!(command, Command::Model(ModelCommand::Manifest));
    }

    #[test]
    fn parses_model_inspect() {
        let command = parse([
            "model".to_string(),
            "inspect".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Model(ModelCommand::Inspect {
                id: "qwen3.5-4b".to_string()
            })
        );
    }

    #[test]
    fn parses_model_registry() {
        let command = parse(["model".to_string(), "registry".to_string()]).unwrap();
        assert_eq!(command, Command::Model(ModelCommand::Registry));
    }

    #[test]
    fn parses_model_download_plan() {
        let command = parse([
            "model".to_string(),
            "download-plan".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Model(ModelCommand::DownloadPlan {
                id: "qwen3.5-4b".to_string()
            })
        );
    }

    #[test]
    fn parses_model_verify_file() {
        let command = parse([
            "model".to_string(),
            "verify-file".to_string(),
            "model.gguf".to_string(),
            "--sha256".to_string(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Model(ModelCommand::VerifyFile {
                path: "model.gguf".to_string(),
                sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                    .to_string()
            })
        );
    }

    #[test]
    fn parses_model_cleanup_failed_dry_run() {
        let command = parse([
            "model".to_string(),
            "cleanup-failed".to_string(),
            "qwen3.5-4b".to_string(),
            "--dry-run".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Model(ModelCommand::CleanupFailed {
                id: "qwen3.5-4b".to_string(),
                dry_run: true
            })
        );
    }

    #[test]
    fn parses_backend_doctor() {
        let command = parse(["backend".to_string(), "doctor".to_string()]).unwrap();
        assert_eq!(command, Command::Backend(BackendCommand::Doctor));
    }

    #[test]
    fn parses_backend_install_plan() {
        let command = parse(["backend".to_string(), "install-plan".to_string()]).unwrap();
        assert_eq!(command, Command::Backend(BackendCommand::InstallPlan));
    }

    #[test]
    fn parses_backend_verify_archive() {
        let command = parse([
            "backend".to_string(),
            "verify-archive".to_string(),
            "llama.zip".to_string(),
            "--sha256".to_string(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Backend(BackendCommand::VerifyArchive {
                path: "llama.zip".to_string(),
                sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                    .to_string()
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

    #[test]
    fn parses_monitor_status() {
        let command = parse(["monitor".to_string(), "status".to_string()]).unwrap();
        assert_eq!(command, Command::Monitor(MonitorCommand::Status));
    }

    #[test]
    fn parses_state_reconcile() {
        let command = parse(["state".to_string(), "reconcile".to_string()]).unwrap();
        assert_eq!(command, Command::State(StateCommand::Reconcile));
    }

    #[test]
    fn parses_state_resume() {
        let command = parse(["state".to_string(), "resume".to_string()]).unwrap();
        assert_eq!(command, Command::State(StateCommand::Resume));
    }

    #[test]
    fn parses_evidence_validate() {
        let command = parse([
            "evidence".to_string(),
            "validate".to_string(),
            "logs/test.log".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Evidence(EvidenceCommand::Validate {
                pointer: "logs/test.log".to_string()
            })
        );
    }

    #[test]
    fn parses_skill_run() {
        let command = parse([
            "skill".to_string(),
            "run".to_string(),
            "fix-test".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Skill(SkillCommand::Run {
                id: "fix-test".to_string()
            })
        );
    }

    #[test]
    fn parses_run_request() {
        let command = parse([
            "run".to_string(),
            "테스트".to_string(),
            "고쳐줘".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Run {
                request: "테스트 고쳐줘".to_string()
            }
        );
    }

    #[test]
    fn parses_intent_classify_request() {
        let command = parse([
            "intent".to_string(),
            "classify".to_string(),
            "리뷰해줘".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Intent(IntentCommand::Classify {
                request: "리뷰해줘".to_string()
            })
        );
    }

    #[test]
    fn parses_intent_routes() {
        let command = parse(["intent".to_string(), "routes".to_string()]).unwrap();
        assert_eq!(command, Command::Intent(IntentCommand::Routes));
    }

    #[test]
    fn parses_policy_check_command() {
        let command = parse([
            "policy".to_string(),
            "check-command".to_string(),
            "cargo".to_string(),
            "test".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Policy(PolicyCommand::CheckCommand {
                command: "cargo test".to_string()
            })
        );
    }

    #[test]
    fn parses_policy_check_path_write() {
        let command = parse([
            "policy".to_string(),
            "check-path".to_string(),
            "--write".to_string(),
            "src/main.rs".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Policy(PolicyCommand::CheckPath {
                mode: PolicyPathMode::Write,
                path: "src/main.rs".to_string()
            })
        );
    }

    #[test]
    fn parses_hooks_list() {
        let command = parse(["hooks".to_string(), "list".to_string()]).unwrap();
        assert_eq!(command, Command::Hooks(HooksCommand::List));
    }

    #[test]
    fn parses_monitor_export_jsonl() {
        let command = parse([
            "monitor".to_string(),
            "export".to_string(),
            "--format".to_string(),
            "jsonl".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Monitor(MonitorCommand::Export {
                format: MonitorExportFormat::Jsonl
            })
        );
    }

    #[test]
    fn parses_monitor_prune_dry_run() {
        let command = parse([
            "monitor".to_string(),
            "prune".to_string(),
            "--before".to_string(),
            "30d".to_string(),
            "--dry-run".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Monitor(MonitorCommand::Prune {
                before_days: 30,
                dry_run: true
            })
        );
    }

    #[test]
    fn parses_uninstall_dry_run_purge_cache() {
        let command = parse([
            "uninstall".to_string(),
            "--dry-run".to_string(),
            "--purge-cache".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Uninstall(UninstallCommand::Plan {
                purge_cache: true,
                dry_run: true
            })
        );
    }
}
