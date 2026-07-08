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
  rpotato session list
  rpotato session history
  rpotato session resume <session-id>
  rpotato session new
  rpotato team status
  rpotato team admit --lanes <count> [--write <path>] [--command <command>]
  rpotato resume [session-id]
  rpotato tui
  rpotato tui monitor
  rpotato tui sessions
  rpotato tui transcript <session-id>
  rpotato tui approvals
  rpotato tui diff <proposal-id>
  rpotato tui evidence
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
  rpotato patch preview --path <path> --find <text> --replace <text>
  rpotato patch approve <proposal-id> --token <token> [--dry-run] [--verify-command <command>]
  rpotato backend doctor
  rpotato backend install-plan
  rpotato backend install
  rpotato backend start --model <path> [--ctx-size <tokens>]
  rpotato backend status
  rpotato backend stop
  rpotato backend verify-archive <path> --sha256 <hash>
  rpotato backend health-check
  rpotato backend chat --prompt <text> [--max-tokens <tokens>]
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
  rpotato model eval-plan <id>
  rpotato model benchmark-plan <id>
  rpotato model fetch-candidate <id> --for-evaluation
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
  backend install은 source-backed manifest와 SHA-256 검증을 거친 뒤 관리형 release payload를 배치합니다.
  backend start/status/stop/chat은 명시 모델 파일 기준의 managed sidecar lifecycle과 non-streaming chat smoke를 다룹니다.
  team status는 최신 resource sample 기준의 read-only admission preview와 sequential fallback 결정을 표시합니다.
  team admit은 dispatcher 진입 전 resource/policy admission gate를 강제하고 결과를 ledger에 기록합니다.
  모델 registry install은 verified 전까지 차단되며, 검증용 artifact fetch는 --for-evaluation을 요구합니다.";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Init,
    Run { request: String },
    Intent(IntentCommand),
    Doctor,
    Config,
    State(StateCommand),
    Session(SessionCommand),
    Team(TeamCommand),
    Tui(TuiCommand),
    Cancel,
    Evidence(EvidenceCommand),
    Skill(SkillCommand),
    Policy(PolicyCommand),
    Hooks(HooksCommand),
    Patch(PatchCommand),
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
pub enum SessionCommand {
    List,
    New,
    Resume { id: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum TeamCommand {
    Status,
    Admit {
        lanes: u32,
        write_paths: Vec<String>,
        commands: Vec<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum TuiCommand {
    Overview,
    Monitor,
    Sessions,
    Transcript { session_id: String },
    Approvals,
    Diff { proposal_id: String },
    Evidence,
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
pub enum PatchCommand {
    Preview {
        path: String,
        find: String,
        replace: String,
    },
    Approve {
        proposal_id: String,
        token: String,
        dry_run: bool,
        verify_command: Option<String>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum BackendCommand {
    Doctor,
    InstallPlan,
    Install,
    Start {
        model_path: String,
        ctx_size: Option<u32>,
    },
    Status,
    Stop,
    VerifyArchive {
        path: String,
        sha256: String,
    },
    HealthCheck,
    Chat {
        prompt: String,
        max_tokens: Option<u32>,
    },
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
    EvalPlan { id: String },
    BenchmarkPlan { id: String },
    FetchCandidate { id: String },
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
        [arg] if arg == "resume" => Ok(Command::Session(SessionCommand::List)),
        [arg, id] if arg == "resume" => Ok(Command::Session(SessionCommand::Resume {
            id: id.clone(),
        })),
        [arg, ..] if arg == "resume" => Err(AppError::usage(
            "resume은 인자 없이 session history를 보거나 resume <session-id> 형식만 허용합니다.",
        )),
        [group, action] if group == "session" && (action == "list" || action == "history") => {
            Ok(Command::Session(SessionCommand::List))
        }
        [group, action] if group == "session" && action == "new" => {
            Ok(Command::Session(SessionCommand::New))
        }
        [group, action, id] if group == "session" && action == "resume" => {
            Ok(Command::Session(SessionCommand::Resume { id: id.clone() }))
        }
        [group, action, ..] if group == "session" && action == "resume" => Err(
            AppError::usage("session resume에는 session id가 필요합니다."),
        ),
        [group, ..] if group == "session" => Err(AppError::usage(
            "session 명령은 list, history, new, resume만 허용합니다.",
        )),
        [group, action] if group == "team" && action == "status" => {
            Ok(Command::Team(TeamCommand::Status))
        }
        [group, action, rest @ ..] if group == "team" && action == "admit" => {
            Ok(Command::Team(parse_team_admit_args(rest)?))
        }
        [group, ..] if group == "team" => {
            Err(AppError::usage("team 명령은 status, admit만 허용합니다."))
        }
        [arg] if arg == "tui" => Ok(Command::Tui(TuiCommand::Overview)),
        [group, action] if group == "tui" && action == "monitor" => {
            Ok(Command::Tui(TuiCommand::Monitor))
        }
        [group, action] if group == "tui" && action == "sessions" => {
            Ok(Command::Tui(TuiCommand::Sessions))
        }
        [group, action, session_id] if group == "tui" && action == "transcript" => {
            Ok(Command::Tui(TuiCommand::Transcript {
                session_id: session_id.clone(),
            }))
        }
        [group, action, ..] if group == "tui" && action == "transcript" => Err(
            AppError::usage("tui transcript에는 session id가 필요합니다."),
        ),
        [group, action] if group == "tui" && action == "approvals" => {
            Ok(Command::Tui(TuiCommand::Approvals))
        }
        [group, action, proposal_id] if group == "tui" && action == "diff" => {
            Ok(Command::Tui(TuiCommand::Diff {
                proposal_id: proposal_id.clone(),
            }))
        }
        [group, action, ..] if group == "tui" && action == "diff" => Err(AppError::usage(
            "tui diff에는 proposal id가 필요합니다.",
        )),
        [group, action] if group == "tui" && action == "evidence" => {
            Ok(Command::Tui(TuiCommand::Evidence))
        }
        [group, ..] if group == "tui" => Err(AppError::usage(
            "tui 명령은 인자 없음, monitor, sessions, transcript, approvals, diff, evidence만 허용합니다.",
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
        [group, action, rest @ ..] if group == "patch" && action == "preview" => {
            parse_patch_preview(rest).map(Command::Patch)
        }
        [group, action, rest @ ..] if group == "patch" && action == "approve" => {
            parse_patch_approve(rest).map(Command::Patch)
        }
        [group, ..] if group == "patch" => Err(AppError::usage(
            "patch 명령은 preview 또는 approve만 허용합니다.",
        )),
        [group, action] if group == "backend" && action == "doctor" => {
            Ok(Command::Backend(BackendCommand::Doctor))
        }
        [group, action] if group == "backend" && action == "install-plan" => {
            Ok(Command::Backend(BackendCommand::InstallPlan))
        }
        [group, action] if group == "backend" && action == "install" => {
            Ok(Command::Backend(BackendCommand::Install))
        }
        [group, action, rest @ ..] if group == "backend" && action == "start" => {
            parse_backend_start(rest).map(Command::Backend)
        }
        [group, action] if group == "backend" && action == "status" => {
            Ok(Command::Backend(BackendCommand::Status))
        }
        [group, action] if group == "backend" && action == "stop" => {
            Ok(Command::Backend(BackendCommand::Stop))
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
        [group, action] if group == "backend" && action == "health-check" => {
            Ok(Command::Backend(BackendCommand::HealthCheck))
        }
        [group, action, rest @ ..] if group == "backend" && action == "chat" => {
            parse_backend_chat(rest).map(Command::Backend)
        }
        [group, ..] if group == "backend" => Err(AppError::usage(
            "backend 명령은 doctor, install-plan, install, start, status, stop, verify-archive, health-check, chat만 허용합니다.",
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
        [group, action, id] if group == "model" && action == "eval-plan" => {
            Ok(Command::Model(ModelCommand::EvalPlan { id: id.clone() }))
        }
        [group, action, id] if group == "model" && action == "benchmark-plan" => {
            Ok(Command::Model(ModelCommand::BenchmarkPlan { id: id.clone() }))
        }
        [group, action, id, flag]
            if group == "model" && action == "fetch-candidate" && flag == "--for-evaluation" =>
        {
            Ok(Command::Model(ModelCommand::FetchCandidate {
                id: id.clone(),
            }))
        }
        [group, action, ..] if group == "model" && action == "fetch-candidate" => Err(
            AppError::usage(
                "model fetch-candidate는 <id> --for-evaluation 형식이 필요합니다.",
            ),
        ),
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
            "model 명령은 list, manifest, inspect, registry, download-plan, eval-plan, benchmark-plan, fetch-candidate, verify-file, cleanup-failed, install만 허용합니다.",
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

fn parse_team_admit_args(args: &[String]) -> Result<TeamCommand, AppError> {
    let mut lanes = None;
    let mut write_paths = Vec::new();
    let mut commands = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--lanes" => {
                if lanes.is_some() {
                    return Err(AppError::usage(
                        "team admit의 --lanes 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(AppError::usage(
                        "team admit은 --lanes <count> 값이 필요합니다.",
                    ));
                };
                let parsed = value.parse::<u32>().map_err(|_| {
                    AppError::usage("team admit의 --lanes 값은 양의 정수여야 합니다.")
                })?;
                if parsed == 0 {
                    return Err(AppError::usage(
                        "team admit의 --lanes 값은 1 이상이어야 합니다.",
                    ));
                }
                lanes = Some(parsed);
                index += 1;
            }
            "--write" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err(AppError::usage(
                        "team admit은 --write <path> 값이 필요합니다.",
                    ));
                };
                if value.starts_with("--") {
                    return Err(AppError::usage(
                        "team admit은 --write <path> 값이 필요합니다.",
                    ));
                }
                write_paths.push(value.clone());
                index += 1;
            }
            "--command" => {
                index += 1;
                let start = index;
                while index < args.len() && !args[index].starts_with("--") {
                    index += 1;
                }
                if start == index {
                    return Err(AppError::usage(
                        "team admit은 --command <command> 값이 필요합니다.",
                    ));
                }
                commands.push(args[start..index].join(" "));
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 team admit 옵션입니다: {unknown}"
                )));
            }
        }
    }

    Ok(TeamCommand::Admit {
        lanes: lanes
            .ok_or_else(|| AppError::usage("team admit은 --lanes <count> 형식이 필요합니다."))?,
        write_paths,
        commands,
    })
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

fn parse_backend_start(args: &[String]) -> Result<BackendCommand, AppError> {
    let mut model_path = None;
    let mut ctx_size = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--model" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend start는 --model <path> 값이 필요합니다.",
                    ));
                };
                if model_path.is_some() {
                    return Err(AppError::usage(
                        "backend start의 --model 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                model_path = Some(value.clone());
                index += 2;
            }
            "--ctx-size" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend start는 --ctx-size <tokens> 값이 필요합니다.",
                    ));
                };
                if ctx_size.is_some() {
                    return Err(AppError::usage(
                        "backend start의 --ctx-size 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                ctx_size = Some(parse_positive_u32(value, "ctx-size")?);
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 backend start 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let Some(model_path) = model_path else {
        return Err(AppError::usage(
            "backend start는 --model <path> 형식이 필요합니다.",
        ));
    };

    Ok(BackendCommand::Start {
        model_path,
        ctx_size,
    })
}

fn parse_patch_preview(args: &[String]) -> Result<PatchCommand, AppError> {
    let mut path = None;
    let mut find = None;
    let mut replace = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--path" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch preview는 --path <path> 값이 필요합니다.",
                    ));
                };
                path = Some(value.clone());
                index += 2;
            }
            "--find" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch preview는 --find <text> 값이 필요합니다.",
                    ));
                };
                find = Some(value.clone());
                index += 2;
            }
            "--replace" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch preview는 --replace <text> 값이 필요합니다.",
                    ));
                };
                replace = Some(value.clone());
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 patch preview 옵션입니다: {unknown}"
                )));
            }
        }
    }

    Ok(PatchCommand::Preview {
        path: path.ok_or_else(|| AppError::usage("patch preview는 --path가 필요합니다."))?,
        find: find.ok_or_else(|| AppError::usage("patch preview는 --find가 필요합니다."))?,
        replace: replace
            .ok_or_else(|| AppError::usage("patch preview는 --replace가 필요합니다."))?,
    })
}

fn parse_patch_approve(args: &[String]) -> Result<PatchCommand, AppError> {
    let mut proposal_id = None;
    let mut token = None;
    let mut dry_run = false;
    let mut verify_command = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--token" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch approve는 --token <token> 값이 필요합니다.",
                    ));
                };
                token = Some(value.clone());
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--verify-command" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch approve는 --verify-command <command> 값이 필요합니다.",
                    ));
                };
                if verify_command.is_some() {
                    return Err(AppError::usage(
                        "patch approve verification command는 하나만 지정할 수 있습니다.",
                    ));
                }
                verify_command = Some(value.clone());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(AppError::usage(format!(
                    "알 수 없는 patch approve 옵션입니다: {value}"
                )));
            }
            value => {
                if proposal_id.is_some() {
                    return Err(AppError::usage(
                        "patch approve proposal id는 하나만 지정할 수 있습니다.",
                    ));
                }
                proposal_id = Some(value.to_string());
                index += 1;
            }
        }
    }

    let Some(proposal_id) = proposal_id else {
        return Err(AppError::usage(
            "patch approve에는 proposal id가 필요합니다.",
        ));
    };
    let Some(token) = token else {
        return Err(AppError::usage(
            "patch approve는 --token <token> 값이 필요합니다.",
        ));
    };

    Ok(PatchCommand::Approve {
        proposal_id,
        token,
        dry_run,
        verify_command,
    })
}

fn parse_backend_chat(args: &[String]) -> Result<BackendCommand, AppError> {
    let mut prompt = None;
    let mut max_tokens = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--prompt" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend chat은 --prompt <text> 값이 필요합니다.",
                    ));
                };
                if prompt.is_some() {
                    return Err(AppError::usage(
                        "backend chat의 --prompt 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                prompt = Some(value.clone());
                index += 2;
            }
            "--max-tokens" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend chat은 --max-tokens <tokens> 값이 필요합니다.",
                    ));
                };
                if max_tokens.is_some() {
                    return Err(AppError::usage(
                        "backend chat의 --max-tokens 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                max_tokens = Some(parse_positive_u32(value, "max-tokens")?);
                index += 2;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 backend chat 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let Some(prompt) = prompt else {
        return Err(AppError::usage(
            "backend chat은 --prompt <text> 형식이 필요합니다.",
        ));
    };

    Ok(BackendCommand::Chat { prompt, max_tokens })
}

fn parse_positive_u32(value: &str, label: &str) -> Result<u32, AppError> {
    let parsed = value.parse::<u32>().map_err(|_| {
        AppError::usage(format!(
            "{label} 값은 양의 정수여야 합니다. 예: --{label} 4096"
        ))
    })?;
    if parsed == 0 {
        return Err(AppError::usage(format!(
            "{label} 값은 1 이상이어야 합니다."
        )));
    }
    Ok(parsed)
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
    fn parses_model_eval_plan() {
        let command = parse([
            "model".to_string(),
            "eval-plan".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Model(ModelCommand::EvalPlan {
                id: "qwen3.5-4b".to_string()
            })
        );
    }

    #[test]
    fn parses_model_benchmark_plan() {
        let command = parse([
            "model".to_string(),
            "benchmark-plan".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Model(ModelCommand::BenchmarkPlan {
                id: "qwen3.5-4b".to_string()
            })
        );
    }

    #[test]
    fn parses_model_fetch_candidate_for_evaluation() {
        let command = parse([
            "model".to_string(),
            "fetch-candidate".to_string(),
            "qwen3.5-4b".to_string(),
            "--for-evaluation".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Model(ModelCommand::FetchCandidate {
                id: "qwen3.5-4b".to_string()
            })
        );
    }

    #[test]
    fn model_fetch_candidate_requires_evaluation_flag() {
        let err = parse([
            "model".to_string(),
            "fetch-candidate".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap_err();

        assert_eq!(err.code, 2);
        assert!(err.message.contains("--for-evaluation"));
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
    fn parses_backend_install() {
        let command = parse(["backend".to_string(), "install".to_string()]).unwrap();
        assert_eq!(command, Command::Backend(BackendCommand::Install));
    }

    #[test]
    fn parses_backend_start() {
        let command = parse([
            "backend".to_string(),
            "start".to_string(),
            "--model".to_string(),
            "model.gguf".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Backend(BackendCommand::Start {
                model_path: "model.gguf".to_string(),
                ctx_size: None
            })
        );
    }

    #[test]
    fn parses_backend_start_with_ctx_size() {
        let command = parse([
            "backend".to_string(),
            "start".to_string(),
            "--model".to_string(),
            "model.gguf".to_string(),
            "--ctx-size".to_string(),
            "4096".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Backend(BackendCommand::Start {
                model_path: "model.gguf".to_string(),
                ctx_size: Some(4096)
            })
        );
    }

    #[test]
    fn rejects_zero_backend_ctx_size() {
        let err = parse([
            "backend".to_string(),
            "start".to_string(),
            "--model".to_string(),
            "model.gguf".to_string(),
            "--ctx-size".to_string(),
            "0".to_string(),
        ])
        .unwrap_err();

        assert_eq!(err.code, 2);
        assert!(err.message.contains("1 이상"));
    }

    #[test]
    fn parses_backend_status() {
        let command = parse(["backend".to_string(), "status".to_string()]).unwrap();
        assert_eq!(command, Command::Backend(BackendCommand::Status));
    }

    #[test]
    fn parses_backend_stop() {
        let command = parse(["backend".to_string(), "stop".to_string()]).unwrap();
        assert_eq!(command, Command::Backend(BackendCommand::Stop));
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
    fn parses_backend_health_check() {
        let command = parse(["backend".to_string(), "health-check".to_string()]).unwrap();
        assert_eq!(command, Command::Backend(BackendCommand::HealthCheck));
    }

    #[test]
    fn parses_backend_chat() {
        let command = parse([
            "backend".to_string(),
            "chat".to_string(),
            "--prompt".to_string(),
            "감자는 무엇인가?".to_string(),
            "--max-tokens".to_string(),
            "64".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Backend(BackendCommand::Chat {
                prompt: "감자는 무엇인가?".to_string(),
                max_tokens: Some(64)
            })
        );
    }

    #[test]
    fn backend_chat_requires_prompt() {
        let err = parse(["backend".to_string(), "chat".to_string()]).unwrap_err();

        assert_eq!(err.code, 2);
        assert!(err.message.contains("--prompt"));
    }

    #[test]
    fn parses_patch_preview() {
        let command = parse([
            "patch".to_string(),
            "preview".to_string(),
            "--path".to_string(),
            "src/lib.rs".to_string(),
            "--find".to_string(),
            "old".to_string(),
            "--replace".to_string(),
            "new".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Patch(PatchCommand::Preview {
                path: "src/lib.rs".to_string(),
                find: "old".to_string(),
                replace: "new".to_string()
            })
        );
    }

    #[test]
    fn parses_patch_approve_dry_run() {
        let command = parse([
            "patch".to_string(),
            "approve".to_string(),
            "patch-proposal-abc123".to_string(),
            "--token".to_string(),
            "token123".to_string(),
            "--dry-run".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Patch(PatchCommand::Approve {
                proposal_id: "patch-proposal-abc123".to_string(),
                token: "token123".to_string(),
                dry_run: true,
                verify_command: None
            })
        );
    }

    #[test]
    fn parses_patch_approve_apply_with_verify_command() {
        let command = parse([
            "patch".to_string(),
            "approve".to_string(),
            "patch-proposal-abc123".to_string(),
            "--token".to_string(),
            "token123".to_string(),
            "--verify-command".to_string(),
            "cargo fmt --check".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Patch(PatchCommand::Approve {
                proposal_id: "patch-proposal-abc123".to_string(),
                token: "token123".to_string(),
                dry_run: false,
                verify_command: Some("cargo fmt --check".to_string())
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
    fn parses_session_list() {
        let command = parse(["session".to_string(), "list".to_string()]).unwrap();
        assert_eq!(command, Command::Session(SessionCommand::List));
    }

    #[test]
    fn parses_session_history_alias() {
        let command = parse(["session".to_string(), "history".to_string()]).unwrap();
        assert_eq!(command, Command::Session(SessionCommand::List));
    }

    #[test]
    fn parses_session_resume() {
        let command = parse([
            "session".to_string(),
            "resume".to_string(),
            "session-1".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Session(SessionCommand::Resume {
                id: "session-1".to_string()
            })
        );
    }

    #[test]
    fn parses_team_status() {
        let command = parse(["team".to_string(), "status".to_string()]).unwrap();
        assert_eq!(command, Command::Team(TeamCommand::Status));
    }

    #[test]
    fn parses_team_admit_with_lanes() {
        let command = parse([
            "team".to_string(),
            "admit".to_string(),
            "--lanes".to_string(),
            "3".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Team(TeamCommand::Admit {
                lanes: 3,
                write_paths: Vec::new(),
                commands: Vec::new()
            })
        );
    }

    #[test]
    fn parses_team_admit_policy_preflight() {
        let command = parse([
            "team".to_string(),
            "admit".to_string(),
            "--lanes".to_string(),
            "2".to_string(),
            "--write".to_string(),
            "README.md".to_string(),
            "--command".to_string(),
            "cargo".to_string(),
            "test".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Team(TeamCommand::Admit {
                lanes: 2,
                write_paths: vec!["README.md".to_string()],
                commands: vec!["cargo test".to_string()]
            })
        );
    }

    #[test]
    fn rejects_zero_team_admit_lanes() {
        let err = parse([
            "team".to_string(),
            "admit".to_string(),
            "--lanes".to_string(),
            "0".to_string(),
        ])
        .unwrap_err();
        assert_eq!(err.code, 2);
        assert!(err.message.contains("1 이상"));
    }

    #[test]
    fn parses_top_level_resume_as_history() {
        let command = parse(["resume".to_string()]).unwrap();
        assert_eq!(command, Command::Session(SessionCommand::List));
    }

    #[test]
    fn parses_top_level_resume_with_id() {
        let command = parse(["resume".to_string(), "session-1".to_string()]).unwrap();
        assert_eq!(
            command,
            Command::Session(SessionCommand::Resume {
                id: "session-1".to_string()
            })
        );
    }

    #[test]
    fn parses_tui_overview() {
        let command = parse(["tui".to_string()]).unwrap();
        assert_eq!(command, Command::Tui(TuiCommand::Overview));
    }

    #[test]
    fn parses_tui_monitor() {
        let command = parse(["tui".to_string(), "monitor".to_string()]).unwrap();
        assert_eq!(command, Command::Tui(TuiCommand::Monitor));
    }

    #[test]
    fn parses_tui_sessions() {
        let command = parse(["tui".to_string(), "sessions".to_string()]).unwrap();
        assert_eq!(command, Command::Tui(TuiCommand::Sessions));
    }

    #[test]
    fn parses_tui_transcript() {
        let command = parse([
            "tui".to_string(),
            "transcript".to_string(),
            "session-1".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Tui(TuiCommand::Transcript {
                session_id: "session-1".to_string()
            })
        );
    }

    #[test]
    fn parses_tui_approvals() {
        let command = parse(["tui".to_string(), "approvals".to_string()]).unwrap();
        assert_eq!(command, Command::Tui(TuiCommand::Approvals));
    }

    #[test]
    fn parses_tui_diff() {
        let command = parse([
            "tui".to_string(),
            "diff".to_string(),
            "patch-proposal-abc123".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::Tui(TuiCommand::Diff {
                proposal_id: "patch-proposal-abc123".to_string()
            })
        );
    }

    #[test]
    fn parses_tui_evidence() {
        let command = parse(["tui".to_string(), "evidence".to_string()]).unwrap();
        assert_eq!(command, Command::Tui(TuiCommand::Evidence));
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
