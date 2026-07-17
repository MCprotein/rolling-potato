use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::MAX_CHAT_TIMEOUT_MS;
pub(crate) use crate::surfaces::cli::command::*;
use crate::surfaces::cli::render::HELP;

mod collaboration;
use collaboration::{
    parse_subagent_launch_args, parse_team_admit_args, parse_team_cancel_args,
    parse_team_dispatch_args, parse_team_execute_args, parse_team_governor_args,
    parse_team_plan_args, parse_team_reconcile_args,
};

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
        [arg] if arg == "continue" => Ok(Command::State(StateCommand::Resume)),
        [arg, id] if arg == "continue" => Ok(Command::Session(SessionCommand::Resume {
            id: id.clone(),
        })),
        [arg, ..] if arg == "continue" => Err(AppError::usage(
            "continue는 인자 없이 현재 workflow를 이어가거나 continue <session-id> 형식만 허용합니다.",
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
        [group, action, rest @ ..] if group == "team" && action == "plan" => {
            Ok(Command::Team(parse_team_plan_args(rest)?))
        }
        [group, action, rest @ ..] if group == "team" && action == "execute" => {
            Ok(Command::Team(parse_team_execute_args(rest)?))
        }
        [group, action, rest @ ..] if group == "team" && action == "reconcile" => {
            Ok(Command::Team(parse_team_reconcile_args(rest)?))
        }
        [group, action, rest @ ..] if group == "team" && action == "cancel" => {
            Ok(Command::Team(parse_team_cancel_args(rest)?))
        }
        [group, action, rest @ ..] if group == "team" && action == "admit" => {
            Ok(Command::Team(parse_team_admit_args(rest)?))
        }
        [group, action, rest @ ..] if group == "team" && action == "dispatch" => {
            Ok(Command::Team(parse_team_dispatch_args(rest)?))
        }
        [group, action, rest @ ..] if group == "team" && action == "governor" => {
            Ok(Command::Team(parse_team_governor_args(rest)?))
        }
        [group, ..] if group == "team" => {
            Err(AppError::usage(
                "team 명령은 status, plan, execute, reconcile, cancel, admit, dispatch, governor만 허용합니다.",
            ))
        }
        [group, action, rest @ ..] if group == "subagent" && action == "launch" => {
            parse_subagent_launch_args(rest).map(Command::Subagent)
        }
        [group, action] if group == "subagent" && action == "status" => {
            Ok(Command::Subagent(SubagentCommand::Status { id: None }))
        }
        [group, action, id] if group == "subagent" && action == "status" => {
            Ok(Command::Subagent(SubagentCommand::Status {
                id: Some(id.clone()),
            }))
        }
        [group, action, id] if group == "subagent" && action == "cancel" => {
            Ok(Command::Subagent(SubagentCommand::Cancel { id: id.clone() }))
        }
        [group, action, ..] if group == "subagent" && action == "status" => Err(
            AppError::usage("subagent status는 선택적인 subagent id 하나만 허용합니다."),
        ),
        [group, action, ..] if group == "subagent" && action == "cancel" => Err(
            AppError::usage("subagent cancel에는 subagent id 하나가 필요합니다."),
        ),
        [group, ..] if group == "subagent" => Err(AppError::usage(
            "subagent 명령은 launch, status, cancel만 허용합니다.",
        )),
        [arg] if arg == "tui" => Ok(Command::Tui(TuiCommand::Auto)),
        [group, action] if group == "tui" && action == "interactive" => {
            Ok(Command::Tui(TuiCommand::Interactive))
        }
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
            "tui 명령은 인자 없음, interactive, monitor, sessions, transcript, approvals, diff, evidence만 허용합니다.",
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
        [group, action, id, rest @ ..] if group == "skill" && action == "run" => {
            Ok(Command::Skill(SkillCommand::Run {
                id: id.clone(),
                request: parse_request(rest, "skill run")?,
            }))
        }
        [group, action, ..] if group == "skill" && action == "run" => Err(AppError::usage(
            "skill run에는 skill id와 요청이 필요합니다. 예: rpotato skill run fix-test \"테스트 실패를 고쳐줘\"",
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
        [group, action, rest @ ..] if group == "patch" && action == "verify" => {
            parse_patch_verify(rest).map(Command::Patch)
        }
        [group, action, proposal_id] if group == "patch" && action == "token-rotate" => {
            Ok(Command::Patch(PatchCommand::TokenRotate { proposal_id: proposal_id.clone() }))
        }
        [group, ..] if group == "patch" => Err(AppError::usage(
            "patch 명령은 preview, approve, verify, token-rotate만 허용합니다.",
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
        [group, action] if group == "backend" && action == "cancel" => {
            Ok(Command::Backend(BackendCommand::Cancel))
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
            "backend 명령은 doctor, install-plan, install, start, status, stop, cancel, verify-archive, health-check, chat만 허용합니다.",
        )),
        [group, action] if group == "cache" && action == "status" => Ok(Command::CacheStatus),
        [group, action] if group == "monitor" && action == "status" => {
            Ok(Command::Monitor(MonitorCommand::Status))
        }
        [group, action] if group == "monitor" && action == "models" => {
            Ok(Command::Monitor(MonitorCommand::Models))
        }
        [group, action] if group == "monitor" && action == "baseline" => {
            Ok(Command::Monitor(MonitorCommand::Baseline))
        }
        [group, action] if group == "monitor" && action == "optimize" => {
            Ok(Command::Monitor(MonitorCommand::Optimize))
        }
        [group, action, rest @ ..] if group == "monitor" && action == "export" => {
            parse_monitor_export(rest).map(Command::Monitor)
        }
        [group, action, rest @ ..] if group == "monitor" && action == "prune" => {
            parse_monitor_prune(rest).map(Command::Monitor)
        }
        [group, ..] if group == "monitor" => Err(AppError::usage(
            "monitor 명령은 status, models, baseline, optimize, export, prune만 허용합니다.",
        )),
        [group, action] if group == "ontology" && action == "status" => {
            Ok(Command::Ontology(OntologyCommand::Status))
        }
        [group, action] if group == "ontology" && action == "seed" => {
            Ok(Command::Ontology(OntologyCommand::Seed))
        }
        [group, action] if group == "ontology" && action == "inspect" => {
            Ok(Command::Ontology(OntologyCommand::Inspect))
        }
        [group, action, rest @ ..] if group == "ontology" && action == "context" => {
            parse_ontology_context(rest).map(Command::Ontology)
        }
        [group, action, pointer] if group == "ontology" && action == "reread" => {
            Ok(Command::Ontology(OntologyCommand::Reread {
                pointer: pointer.clone(),
            }))
        }
        [group, action, ..] if group == "ontology" && action == "reread" => Err(
            AppError::usage("ontology reread에는 <source-pointer>가 필요합니다."),
        ),
        [group, action, rest @ ..] if group == "ontology" && action == "export" => {
            parse_ontology_export(rest).map(Command::Ontology)
        }
        [group, action, rest @ ..] if group == "ontology" && action == "import" => {
            parse_ontology_import(rest).map(Command::Ontology)
        }
        [group, ..] if group == "ontology" => Err(AppError::usage(
            "ontology 명령은 status, seed, inspect, context, reread, export, import만 허용합니다.",
        )),
        [group, action, path] if group == "benchmark" && action == "validate" => {
            Ok(Command::Benchmark(BenchmarkCommand::Validate {
                path: path.clone(),
            }))
        }
        [group, action, ..] if group == "benchmark" && action == "validate" => Err(
            AppError::usage("benchmark validate에는 fixture JSON path가 필요합니다."),
        ),
        [group, action, rest @ ..] if group == "benchmark" && action == "record" => {
            parse_benchmark_record(rest).map(Command::Benchmark)
        }
        [group, action, rest @ ..] if group == "benchmark" && action == "run" => {
            parse_benchmark_run(rest).map(Command::Benchmark)
        }
        [group, action, rest @ ..] if group == "benchmark" && action == "report" => {
            parse_benchmark_report(rest).map(Command::Benchmark)
        }
        [group, ..] if group == "benchmark" => Err(AppError::usage(
            "benchmark 명령은 validate, record, run, report만 허용합니다.",
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
        [group, action] if group == "model" && action == "default" => {
            Ok(Command::Model(ModelCommand::Default))
        }
        [group, action, id] if group == "model" && action == "default" => {
            Ok(Command::Model(ModelCommand::SetDefault { id: id.clone() }))
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
        [group, action, id, flag, evidence]
            if group == "model" && action == "promote" && flag == "--evidence" =>
        {
            Ok(Command::Model(ModelCommand::Promote {
                id: id.clone(),
                evidence: evidence.clone(),
            }))
        }
        [group, action, ..] if group == "model" && action == "promote" => Err(
            AppError::usage("model promote는 <id> --evidence <file> 형식이 필요합니다."),
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
            "model 명령은 list, manifest, inspect, registry, default, download-plan, eval-plan, benchmark-plan, fetch-candidate, verify-file, promote, cleanup-failed, install만 허용합니다.",
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

fn parse_ontology_context(args: &[String]) -> Result<OntologyCommand, AppError> {
    match args {
        [flag, rest @ ..] if flag == "--query" => {
            if rest.is_empty() {
                return Err(AppError::usage(
                    "ontology context에는 --query <text> 값이 필요합니다.",
                ));
            }
            Ok(OntologyCommand::Context {
                query: rest.join(" "),
            })
        }
        _ => Err(AppError::usage(
            "ontology context는 --query <text> 형식이 필요합니다.",
        )),
    }
}

fn parse_ontology_export(args: &[String]) -> Result<OntologyCommand, AppError> {
    match args {
        [flag, format] if flag == "--format" => {
            let format = match format.as_str() {
                "json" => OntologyExportFormat::Json,
                "jsonl" => OntologyExportFormat::Jsonl,
                _ => {
                    return Err(AppError::usage(
                        "ontology export format은 json 또는 jsonl만 허용합니다.",
                    ));
                }
            };
            Ok(OntologyCommand::Export { format })
        }
        _ => Err(AppError::usage(
            "ontology export에는 --format json 또는 --format jsonl 형식이 필요합니다.",
        )),
    }
}

fn parse_ontology_import(args: &[String]) -> Result<OntologyCommand, AppError> {
    let mut path = None;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--file" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "ontology import --file에는 path가 필요합니다.",
                    ));
                };
                if path.is_some() {
                    return Err(AppError::usage(
                        "ontology import --file은 한 번만 지정할 수 있습니다.",
                    ));
                }
                path = Some(value.clone());
                index += 2;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            unknown => {
                return Err(AppError::usage(format!(
                    "알 수 없는 ontology import 옵션입니다: {unknown}"
                )));
            }
        }
    }

    let Some(path) = path else {
        return Err(AppError::usage(
            "ontology import에는 --file <path>가 필요합니다.",
        ));
    };
    if !dry_run {
        return Err(AppError::usage(
            "ontology import는 현재 --dry-run을 명시해야 합니다.",
        ));
    }

    Ok(OntologyCommand::Import { path, dry_run })
}

fn parse_benchmark_record(args: &[String]) -> Result<BenchmarkCommand, AppError> {
    match args {
        [flag, fixture] if flag == "--fixture" => Ok(BenchmarkCommand::Record {
            fixture: fixture.clone(),
        }),
        _ => Err(AppError::usage(
            "benchmark record에는 --fixture <fixture.json> 형식이 필요합니다.",
        )),
    }
}

fn parse_benchmark_run(args: &[String]) -> Result<BenchmarkCommand, AppError> {
    let mut fixture = None;
    let mut prompt = None;
    let mut max_tokens = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--fixture" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "benchmark run --fixture에는 fixture path가 필요합니다.",
                    ));
                };
                fixture = Some(value.clone());
                index += 2;
            }
            "--prompt" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "benchmark run --prompt에는 prompt artifact path가 필요합니다.",
                    ));
                };
                prompt = Some(value.clone());
                index += 2;
            }
            "--max-tokens" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "benchmark run --max-tokens에는 양의 정수가 필요합니다.",
                    ));
                };
                let parsed = value.parse::<u32>().map_err(|_| {
                    AppError::usage("benchmark run --max-tokens에는 양의 정수가 필요합니다.")
                })?;
                if parsed == 0 {
                    return Err(AppError::usage(
                        "benchmark run --max-tokens는 1 이상이어야 합니다.",
                    ));
                }
                max_tokens = Some(parsed);
                index += 2;
            }
            _ => {
                return Err(AppError::usage(
                    "benchmark run은 --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>] 형식이 필요합니다.",
                ));
            }
        }
    }

    let Some(fixture) = fixture else {
        return Err(AppError::usage(
            "benchmark run에는 --fixture <fixture.json>이 필요합니다.",
        ));
    };
    let Some(prompt) = prompt else {
        return Err(AppError::usage(
            "benchmark run에는 --prompt <artifact>가 필요합니다.",
        ));
    };

    Ok(BenchmarkCommand::Run {
        fixture,
        prompt,
        max_tokens,
    })
}

fn parse_benchmark_report(args: &[String]) -> Result<BenchmarkCommand, AppError> {
    match args {
        [flag, format] if flag == "--format" => {
            let format = match format.as_str() {
                "jsonl" => BenchmarkReportFormat::Jsonl,
                _ => {
                    return Err(AppError::usage(
                        "benchmark report format은 jsonl만 허용합니다.",
                    ));
                }
            };
            Ok(BenchmarkCommand::Report { format })
        }
        _ => Err(AppError::usage(
            "benchmark report에는 --format jsonl 형식이 필요합니다.",
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
    })
}

fn parse_patch_verify(args: &[String]) -> Result<PatchCommand, AppError> {
    let mut proposal_id = None;
    let mut token = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--token" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "patch verify는 --token <token> 값이 필요합니다.",
                    ));
                };
                token = Some(value.clone());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(AppError::usage(format!(
                    "알 수 없는 patch verify 옵션입니다: {value}"
                )));
            }
            value => {
                if proposal_id.is_some() {
                    return Err(AppError::usage(
                        "patch verify proposal id는 하나만 지정할 수 있습니다.",
                    ));
                }
                proposal_id = Some(value.to_string());
                index += 1;
            }
        }
    }
    Ok(PatchCommand::Verify {
        proposal_id: proposal_id
            .ok_or_else(|| AppError::usage("patch verify에는 proposal id가 필요합니다."))?,
        token: token
            .ok_or_else(|| AppError::usage("patch verify는 --token <token> 값이 필요합니다."))?,
    })
}

fn parse_backend_chat(args: &[String]) -> Result<BackendCommand, AppError> {
    let mut prompt = None;
    let mut max_tokens = None;
    let mut stream = false;
    let mut timeout_ms = None;
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
            "--stream" => {
                if stream {
                    return Err(AppError::usage(
                        "backend chat의 --stream 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                stream = true;
                index += 1;
            }
            "--timeout-ms" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(AppError::usage(
                        "backend chat은 --timeout-ms <ms> 값이 필요합니다.",
                    ));
                };
                if timeout_ms.is_some() {
                    return Err(AppError::usage(
                        "backend chat의 --timeout-ms 옵션은 한 번만 지정할 수 있습니다.",
                    ));
                }
                let value = parse_positive_u32(value, "timeout-ms")?;
                if value > MAX_CHAT_TIMEOUT_MS {
                    return Err(AppError::usage(format!(
                        "backend chat timeout은 1..={} ms 범위여야 합니다.",
                        MAX_CHAT_TIMEOUT_MS
                    )));
                }
                timeout_ms = Some(value);
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

    Ok(BackendCommand::Chat {
        prompt,
        max_tokens,
        stream,
        timeout_ms,
    })
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
#[path = "parser/tests/mod.rs"]
mod tests;
