use crate::foundation::error::AppError;
pub(crate) use crate::surfaces::cli::command::*;
use crate::surfaces::cli::render::HELP;

mod backend;
mod collaboration;
mod governance;
mod install;
mod lifecycle;
mod model;
mod observability;
mod patch;
mod plugin;
mod uninstall;
use backend::{parse_backend_chat, parse_backend_start};
use collaboration::{
    parse_subagent_launch_args, parse_team_admit_args, parse_team_cancel_args,
    parse_team_dispatch_args, parse_team_execute_args, parse_team_governor_args,
    parse_team_plan_args, parse_team_reconcile_args,
};
use governance::{parse_evidence, parse_hooks, parse_policy, parse_skill};
use install::parse_install;
use lifecycle::{
    parse_continue, parse_resume, parse_session, parse_state, parse_tui, parse_update,
};
use model::parse_model;
use observability::{
    parse_benchmark_record, parse_benchmark_report, parse_benchmark_run, parse_monitor_export,
    parse_monitor_prune, parse_ontology_context, parse_ontology_export, parse_ontology_import,
};
use patch::{parse_patch_approve, parse_patch_preview, parse_patch_verify};
use plugin::parse_plugin_import;
use uninstall::parse_uninstall;

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Command, AppError> {
    let args: Vec<String> = args.into_iter().collect();

    match args.as_slice() {
        [] => Ok(Command::Tui(TuiCommand::Auto)),
        [arg] if arg == "help" || arg == "--help" || arg == "-h" => Ok(Command::Help),
        [arg] if arg == "debug" => Ok(Command::AdvancedHelp),
        [group, arg]
            if group == "debug" && (arg == "help" || arg == "--help" || arg == "-h") =>
        {
            Ok(Command::AdvancedHelp)
        }
        [group, rest @ ..] if group == "debug" => parse(rest.iter().cloned()),
        [group, rest @ ..] if group == "install" => {
            parse_install(rest).map(Command::Install)
        }
        [group, rest @ ..] if group == "update" => {
            parse_update(rest).map(Command::Update)
        }
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
        [group, rest @ ..] if group == "state" => parse_state(rest).map(Command::State),
        [group, rest @ ..] if group == "resume" => parse_resume(rest),
        [group, rest @ ..] if group == "continue" => parse_continue(rest),
        [group, rest @ ..] if group == "session" => {
            parse_session(rest).map(Command::Session)
        }
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
        [group, rest @ ..] if group == "tui" => parse_tui(rest).map(Command::Tui),
        [arg] if arg == "cancel" => Ok(Command::Cancel),
        [group, rest @ ..] if group == "evidence" => {
            parse_evidence(rest).map(Command::Evidence)
        }
        [group, rest @ ..] if group == "skill" => parse_skill(rest).map(Command::Skill),
        [group, rest @ ..] if group == "policy" => {
            parse_policy(rest).map(Command::Policy)
        }
        [group, rest @ ..] if group == "hooks" => parse_hooks(rest).map(Command::Hooks),
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
        [group, rest @ ..] if group == "model" => parse_model(rest).map(Command::Model),
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

#[cfg(test)]
#[path = "parser/tests/mod.rs"]
mod tests;
