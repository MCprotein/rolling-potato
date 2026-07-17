use crate::adapters::filesystem::cache;
use crate::adapters::filesystem::layout as paths;
use crate::adapters::terminal::{capability, native};
use crate::composition::{config, dispatch, inference, uninstall};
use crate::evidence;
use crate::foundation::error::AppError;
use crate::hooks;
use crate::intent;
use crate::ontology;
use crate::patch;
use crate::plugin;
use crate::policy;
use crate::runtime;
use crate::skill;
use crate::state;
use crate::subagent;
use crate::surfaces::cli::{
    command::{
        Command, EvidenceCommand, HooksCommand, IntentCommand, MonitorCommand, OntologyCommand,
        PatchCommand, PluginCommand, PolicyCommand, PolicyPathMode, SessionCommand, SkillCommand,
        StateCommand, SubagentCommand, TeamCommand, TuiCommand, UninstallCommand,
    },
    render,
};
use crate::team;
use crate::tui;

use super::inference_adapter::{backend, benchmark, model};
use super::monitor_adapter as monitor;

pub(super) struct LegacyCommandDispatchPort;

impl inference::BenchmarkCommandPort for LegacyCommandDispatchPort {
    fn validate_report(&mut self, path: &str) -> Result<String, AppError> {
        benchmark::validate_report(path)
    }

    fn record_report(&mut self, fixture: &str) -> Result<String, AppError> {
        benchmark::record_report(fixture)
    }

    fn run_report(
        &mut self,
        fixture: &str,
        prompt: &str,
        max_tokens: Option<u32>,
    ) -> Result<String, AppError> {
        benchmark::run_report(fixture, prompt, max_tokens)
    }

    fn report_export(
        &mut self,
        format: crate::surfaces::cli::command::BenchmarkReportFormat,
    ) -> Result<String, AppError> {
        benchmark::report_export(format)
    }
}

impl inference::BackendCommandPort for LegacyCommandDispatchPort {
    fn doctor_report(&mut self) -> String {
        backend::doctor_report()
    }

    fn install_plan_report(&mut self) -> String {
        backend::install_plan_report()
    }

    fn install_report(&mut self) -> Result<String, AppError> {
        backend::install_report()
    }

    fn default_model_path(&mut self) -> Result<String, AppError> {
        Ok(model::default_artifact_path()?.display().to_string())
    }

    fn start_report(
        &mut self,
        model_path: &str,
        ctx_size: Option<u32>,
    ) -> Result<String, AppError> {
        backend::start_report(model_path, ctx_size)
    }

    fn status_report(&mut self) -> Result<String, AppError> {
        backend::status_report()
    }

    fn stop_report(&mut self) -> Result<String, AppError> {
        backend::stop_report()
    }

    fn cancel_generation_report(&mut self) -> Result<String, AppError> {
        backend::cancel_generation_report()
    }

    fn verify_archive_report(&mut self, path: &str, sha256: &str) -> Result<String, AppError> {
        backend::verify_archive_report(path, sha256)
    }

    fn health_check_report(&mut self) -> String {
        backend::health_check_report()
    }

    fn chat_report(
        &mut self,
        prompt: &str,
        max_tokens: Option<u32>,
        timeout_ms: Option<u32>,
    ) -> Result<String, AppError> {
        backend::chat_report(prompt, max_tokens, timeout_ms)
    }

    fn chat_stream_report(
        &mut self,
        prompt: &str,
        max_tokens: Option<u32>,
        timeout_ms: Option<u32>,
        writer: &mut impl std::io::Write,
    ) -> Result<String, AppError> {
        backend::chat_stream_report(prompt, max_tokens, timeout_ms, writer)
    }
}

impl inference::ModelCommandPort for LegacyCommandDispatchPort {
    fn list_report(&mut self) -> String {
        model::list_report()
    }

    fn manifest_report(&mut self) -> String {
        model::manifest_report()
    }

    fn inspect_report(&mut self, id: &str) -> Result<String, AppError> {
        model::inspect_report(id)
    }

    fn registry_report(&mut self) -> String {
        model::registry_report()
    }

    fn default_report(&mut self) -> Result<String, AppError> {
        model::default_report()
    }

    fn set_default_report(&mut self, id: &str) -> Result<String, AppError> {
        model::set_default_report(id)
    }

    fn download_plan_report(&mut self, id: &str) -> Result<String, AppError> {
        model::download_plan_report(id)
    }

    fn eval_plan_report(&mut self, id: &str) -> Result<String, AppError> {
        model::eval_plan_report(id)
    }

    fn benchmark_plan_report(&mut self, id: &str) -> Result<String, AppError> {
        model::benchmark_plan_report(id)
    }

    fn fetch_candidate_report(&mut self, id: &str) -> Result<String, AppError> {
        model::fetch_candidate_for_evaluation_report(id)
    }

    fn verify_file_report(&mut self, path: &str, sha256: &str) -> Result<String, AppError> {
        model::verify_file_report(path, sha256)
    }

    fn promote_candidate_report(&mut self, id: &str, evidence: &str) -> Result<String, AppError> {
        model::promote_candidate_report(id, evidence)
    }

    fn cleanup_failed_report(&mut self, id: &str, dry_run: bool) -> Result<String, AppError> {
        model::cleanup_failed_report(id, dry_run)
    }

    fn install_candidate(&mut self, id: &str) -> Result<(), AppError> {
        model::install_candidate(id)
    }
}

fn emit_inference_output(output: inference::CommandOutput) {
    match output {
        inference::CommandOutput::Line(report) => println!("{report}"),
        inference::CommandOutput::Exact(report) => print!("{report}"),
        inference::CommandOutput::None => {}
    }
}

impl dispatch::CommandDispatchPort for LegacyCommandDispatchPort {
    fn terminal_attached(&mut self) -> bool {
        capability::attached()
    }

    fn validate_native_terminal(&mut self) -> Result<(), AppError> {
        native::validate_native_fault_configuration().map_err(crate::tui::terminal_fault_error)
    }

    fn recover_pending_source_bundles(&mut self) -> Result<(), AppError> {
        crate::transition::recover_pending_source_bundles().map(|_| ())
    }

    fn execute(&mut self, command: Command) -> Result<(), AppError> {
        match command {
            Command::Help => {
                println!("{}", render::HELP);
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
            Command::Backend(command) => {
                let stdout = std::io::stdout();
                let mut writer = stdout.lock();
                let output = inference::run_backend(command, self, &mut writer)?;
                drop(writer);
                emit_inference_output(output);
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
            Command::Benchmark(command) => {
                emit_inference_output(inference::run_benchmark(command, self)?);
                Ok(())
            }
            Command::Model(command) => {
                emit_inference_output(inference::run_model(command, self)?);
                Ok(())
            }
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
}
