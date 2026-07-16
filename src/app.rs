use crate::adapters::filesystem::cache;
use crate::adapters::filesystem::layout as paths;
use crate::adapters::terminal::{capability, native};
use crate::backend;
use crate::benchmark;
use crate::cli::{
    BackendCommand, BenchmarkCommand, Command, EvidenceCommand, HooksCommand, IntentCommand,
    ModelCommand, MonitorCommand, OntologyCommand, PatchCommand, PluginCommand, PolicyCommand,
    PolicyPathMode, SessionCommand, SkillCommand, StateCommand, SubagentCommand, TeamCommand,
    TuiCommand, UninstallCommand,
};
use crate::composition::config;
use crate::evidence;
use crate::foundation::error::AppError;
use crate::hooks;
use crate::intent;
use crate::model;
use crate::monitor;
use crate::ontology;
use crate::patch;
use crate::plugin;
use crate::policy;
use crate::runtime;
use crate::skill;
use crate::state;
use crate::subagent;
use crate::team;
use crate::tui;
use crate::uninstall;

pub fn run(args: impl IntoIterator<Item = String>) -> Result<(), AppError> {
    let command = crate::cli::parse(args)?;
    if matches!(&command, Command::Tui(TuiCommand::Interactive))
        || (matches!(&command, Command::Tui(TuiCommand::Auto)) && capability::attached())
    {
        native::validate_native_fault_configuration().map_err(crate::tui::terminal_fault_error)?;
    }
    // A source-install request on an unsupported platform is a strict
    // NotDispatched boundary: do not even discover or repair journals before
    // returning the typed platform result. Other commands recover through the
    // ordinary startup path (and mutating TUI actions also recover under their
    // transition guard).
    let unsupported_source_entry = !cfg!(unix)
        && (matches!(
            &command,
            Command::Patch(PatchCommand::Approve { dry_run: false, .. })
        ) || matches!(&command, Command::Tui(TuiCommand::Interactive))
            || (matches!(&command, Command::Tui(TuiCommand::Auto)) && capability::attached()));
    if !unsupported_source_entry {
        crate::transition::recover_pending_source_bundles()?;
    }
    match command {
        Command::Help => {
            println!("{}", crate::cli::HELP);
            Ok(())
        }
        Command::Init => {
            println!("{}", runtime::init_report()?);
            Ok(())
        }
        Command::Run { request } => {
            println!("{}", runtime::agent_run_report(&request)?);
            Ok(())
        }
        Command::Intent(IntentCommand::Classify { request }) => {
            println!("{}", intent::classify_report(&request)?);
            Ok(())
        }
        Command::Intent(IntentCommand::Routes) => {
            println!("{}", intent::routes_report());
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
        Command::State(StateCommand::Status) => {
            println!("{}", state::status_report()?);
            Ok(())
        }
        Command::State(StateCommand::Reconcile) => {
            println!("{}", state::reconcile_report()?);
            Ok(())
        }
        Command::State(StateCommand::Resume) => {
            println!("{}", runtime::workflow_resume_report()?);
            Ok(())
        }
        Command::Session(SessionCommand::List) => {
            println!("{}", state::session_list_report()?);
            Ok(())
        }
        Command::Session(SessionCommand::New) => {
            println!("{}", state::session_new_report()?);
            Ok(())
        }
        Command::Session(SessionCommand::Resume { id }) => {
            println!("{}", runtime::session_resume_report(&id)?);
            Ok(())
        }
        Command::Team(TeamCommand::Status) => {
            println!("{}", team::status_report()?);
            Ok(())
        }
        Command::Team(TeamCommand::Plan { manifest_path }) => {
            println!("{}", crate::team_state::plan_report(&manifest_path)?);
            Ok(())
        }
        Command::Team(TeamCommand::Execute { team_id }) => {
            println!("{}", crate::team_execution::execute_report(&team_id)?);
            Ok(())
        }
        Command::Team(TeamCommand::Reconcile { team_id }) => {
            println!(
                "{}",
                crate::team_reconciliation::reconcile_report(&team_id)?
            );
            Ok(())
        }
        Command::Team(TeamCommand::Cancel { team_id }) => {
            println!("{}", crate::team_state::cancel_report(&team_id)?);
            Ok(())
        }
        Command::Team(TeamCommand::Admit {
            lanes,
            write_paths,
            owned_write_paths,
            commands,
        }) => {
            println!(
                "{}",
                team::admission_report(lanes, &write_paths, &owned_write_paths, &commands)?
            );
            Ok(())
        }
        Command::Team(TeamCommand::Dispatch {
            lanes,
            owned_write_paths,
            failed_lane,
            failure_reason,
        }) => {
            println!(
                "{}",
                team::dispatch_report(
                    lanes,
                    &owned_write_paths,
                    failed_lane,
                    failure_reason.as_deref()
                )?
            );
            Ok(())
        }
        Command::Team(TeamCommand::Governor {
            lanes,
            context_tokens,
            context_limit,
            model_tier,
        }) => {
            println!(
                "{}",
                team::governor_report(lanes, context_tokens, context_limit, model_tier)?
            );
            Ok(())
        }
        Command::Subagent(SubagentCommand::Launch {
            role,
            task,
            tools,
            read_paths,
            write_paths,
            timeout_ms,
            max_tokens,
        }) => {
            println!(
                "{}",
                subagent::launch_report(
                    &role,
                    &task,
                    &tools,
                    &read_paths,
                    &write_paths,
                    timeout_ms,
                    max_tokens,
                )?
            );
            Ok(())
        }
        Command::Subagent(SubagentCommand::Status { id }) => {
            println!("{}", subagent::status_report(id.as_deref())?);
            Ok(())
        }
        Command::Subagent(SubagentCommand::Cancel { id }) => {
            println!("{}", subagent::cancel_report(&id)?);
            Ok(())
        }
        Command::Tui(TuiCommand::Auto) => {
            if cfg!(unix) && capability::attached() && !paths::current_state_file().is_file() {
                state::initialize()?;
            }
            tui::run_auto()
        }
        Command::Tui(TuiCommand::Interactive) => {
            if cfg!(unix) && !paths::current_state_file().is_file() {
                state::initialize()?;
            }
            tui::run_interactive()
        }
        Command::Tui(TuiCommand::Monitor) => {
            println!("{}", tui::monitor_report()?);
            Ok(())
        }
        Command::Tui(TuiCommand::Sessions) => {
            println!("{}", tui::sessions_report()?);
            Ok(())
        }
        Command::Tui(TuiCommand::Transcript { session_id }) => {
            println!("{}", tui::transcript_report(&session_id)?);
            Ok(())
        }
        Command::Tui(TuiCommand::Approvals) => {
            println!("{}", tui::approvals_report()?);
            Ok(())
        }
        Command::Tui(TuiCommand::Diff { proposal_id }) => {
            println!("{}", tui::diff_report(&proposal_id)?);
            Ok(())
        }
        Command::Tui(TuiCommand::Evidence) => {
            println!("{}", tui::evidence_report()?);
            Ok(())
        }
        Command::Cancel => {
            println!("{}", state::cancel_report()?);
            Ok(())
        }
        Command::Evidence(EvidenceCommand::Validate { pointer }) => {
            println!("{}", evidence::validate_report(&pointer)?);
            Ok(())
        }
        Command::Skill(SkillCommand::List) => {
            println!("{}", skill::list_report());
            Ok(())
        }
        Command::Skill(SkillCommand::Run { id, request }) => {
            println!("{}", intent::run_skill_report(&id, &request)?);
            Ok(())
        }
        Command::Policy(PolicyCommand::Schema) => {
            println!("{}", policy::schema_report());
            Ok(())
        }
        Command::Policy(PolicyCommand::CheckCommand { command }) => {
            println!("{}", policy::check_command_report(&command)?);
            Ok(())
        }
        Command::Policy(PolicyCommand::CheckPath { mode, path }) => {
            let mode = match mode {
                PolicyPathMode::Read => policy::PathMode::Read,
                PolicyPathMode::Write => policy::PathMode::Write,
            };
            println!("{}", policy::check_path_report(mode, &path)?);
            Ok(())
        }
        Command::Policy(PolicyCommand::Redact { text }) => {
            println!("{}", policy::redact_report(&text));
            Ok(())
        }
        Command::Hooks(HooksCommand::List) => {
            println!("{}", hooks::list_report());
            Ok(())
        }
        Command::Hooks(HooksCommand::ValidateResult { json }) => {
            println!("{}", hooks::validate_result_report(&json)?);
            Ok(())
        }
        Command::Patch(PatchCommand::Preview {
            path,
            find,
            replace,
        }) => {
            println!("{}", patch::preview_report(&path, &find, &replace)?);
            Ok(())
        }
        Command::Patch(PatchCommand::Approve {
            proposal_id,
            token,
            dry_run,
        }) => runtime::patch_approve_to_stdout(&proposal_id, &token, dry_run, None),
        Command::Patch(PatchCommand::Verify { proposal_id, token }) => {
            println!("{}", runtime::patch_verify_report(&proposal_id, &token)?);
            Ok(())
        }
        Command::Patch(PatchCommand::TokenRotate { proposal_id }) => {
            println!("{}", patch::rotate_workflow_token_report(&proposal_id)?);
            Ok(())
        }
        Command::Backend(BackendCommand::Doctor) => {
            println!("{}", backend::doctor_report());
            Ok(())
        }
        Command::Backend(BackendCommand::InstallPlan) => {
            println!("{}", backend::install_plan_report());
            Ok(())
        }
        Command::Backend(BackendCommand::Install) => {
            println!("{}", backend::install_report()?);
            Ok(())
        }
        Command::Backend(BackendCommand::Start {
            model_path,
            ctx_size,
        }) => {
            let model_path = match model_path {
                Some(path) => path,
                None => model::default_artifact_path()?.display().to_string(),
            };
            println!("{}", backend::start_report(&model_path, ctx_size)?);
            Ok(())
        }
        Command::Backend(BackendCommand::Status) => {
            println!("{}", backend::status_report()?);
            Ok(())
        }
        Command::Backend(BackendCommand::Stop) => {
            println!("{}", backend::stop_report()?);
            Ok(())
        }
        Command::Backend(BackendCommand::Cancel) => {
            println!("{}", backend::cancel_generation_report()?);
            Ok(())
        }
        Command::Backend(BackendCommand::VerifyArchive { path, sha256 }) => {
            println!("{}", backend::verify_archive_report(&path, &sha256)?);
            Ok(())
        }
        Command::Backend(BackendCommand::HealthCheck) => {
            println!("{}", backend::health_check_report());
            Ok(())
        }
        Command::Backend(BackendCommand::Chat {
            prompt,
            max_tokens,
            stream,
            timeout_ms,
        }) => {
            if stream {
                let stdout = std::io::stdout();
                let mut writer = stdout.lock();
                let report =
                    backend::chat_stream_report(&prompt, max_tokens, timeout_ms, &mut writer)?;
                drop(writer);
                println!("{report}");
            } else {
                println!("{}", backend::chat_report(&prompt, max_tokens, timeout_ms)?);
            }
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
        Command::Monitor(MonitorCommand::Baseline) => {
            println!("{}", monitor::baseline_report()?);
            Ok(())
        }
        Command::Monitor(MonitorCommand::Optimize) => {
            println!("{}", monitor::optimize_report()?);
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
        Command::Ontology(OntologyCommand::Status) => {
            println!("{}", ontology::status_report()?);
            Ok(())
        }
        Command::Ontology(OntologyCommand::Seed) => {
            println!("{}", ontology::seed_report()?);
            Ok(())
        }
        Command::Ontology(OntologyCommand::Inspect) => {
            println!("{}", ontology::inspect_report()?);
            Ok(())
        }
        Command::Ontology(OntologyCommand::Context { query }) => {
            println!("{}", ontology::context_report(&query)?);
            Ok(())
        }
        Command::Ontology(OntologyCommand::Reread { pointer }) => {
            println!("{}", ontology::reread_report(&pointer)?);
            Ok(())
        }
        Command::Ontology(OntologyCommand::Export { format }) => {
            print!("{}", ontology::export_report(format)?);
            Ok(())
        }
        Command::Ontology(OntologyCommand::Import { path, dry_run }) => {
            println!("{}", ontology::import_report(&path, dry_run)?);
            Ok(())
        }
        Command::Benchmark(BenchmarkCommand::Validate { path }) => {
            println!("{}", benchmark::validate_report(&path)?);
            Ok(())
        }
        Command::Benchmark(BenchmarkCommand::Record { fixture }) => {
            println!("{}", benchmark::record_report(&fixture)?);
            Ok(())
        }
        Command::Benchmark(BenchmarkCommand::Run {
            fixture,
            prompt,
            max_tokens,
        }) => {
            println!("{}", benchmark::run_report(&fixture, &prompt, max_tokens)?);
            Ok(())
        }
        Command::Benchmark(BenchmarkCommand::Report { format }) => {
            print!("{}", benchmark::report_export(format)?);
            Ok(())
        }
        Command::Model(ModelCommand::List) => {
            println!("{}", model::list_report());
            Ok(())
        }
        Command::Model(ModelCommand::Manifest) => {
            println!("{}", model::manifest_report());
            Ok(())
        }
        Command::Model(ModelCommand::Inspect { id }) => {
            println!("{}", model::inspect_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::Registry) => {
            println!("{}", model::registry_report());
            Ok(())
        }
        Command::Model(ModelCommand::Default) => {
            println!("{}", model::default_report()?);
            Ok(())
        }
        Command::Model(ModelCommand::SetDefault { id }) => {
            println!("{}", model::set_default_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::DownloadPlan { id }) => {
            println!("{}", model::download_plan_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::EvalPlan { id }) => {
            println!("{}", model::eval_plan_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::BenchmarkPlan { id }) => {
            println!("{}", model::benchmark_plan_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::FetchCandidate { id }) => {
            println!("{}", model::fetch_candidate_for_evaluation_report(&id)?);
            Ok(())
        }
        Command::Model(ModelCommand::VerifyFile { path, sha256 }) => {
            println!("{}", model::verify_file_report(&path, &sha256)?);
            Ok(())
        }
        Command::Model(ModelCommand::Promote { id, evidence }) => {
            println!("{}", model::promote_candidate_report(&id, &evidence)?);
            Ok(())
        }
        Command::Model(ModelCommand::CleanupFailed { id, dry_run }) => {
            println!("{}", model::cleanup_failed_report(&id, dry_run)?);
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
        Command::Plugin(PluginCommand::Inspect { id }) => {
            println!("{}", plugin::inspect_report(&id)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::Validate { id }) => {
            println!("{}", plugin::validate_report(&id)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::Enable { id }) => {
            println!("{}", plugin::set_enabled_report(&id, true)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::Disable { id }) => {
            println!("{}", plugin::set_enabled_report(&id, false)?);
            Ok(())
        }
        Command::Plugin(PluginCommand::Remove { id, purge_data }) => {
            println!("{}", plugin::remove_report(&id, purge_data)?);
            Ok(())
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
