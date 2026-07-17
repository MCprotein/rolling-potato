use crate::adapters::filesystem::cache;
use crate::adapters::filesystem::layout as paths;
use crate::adapters::terminal::{capability, native};
use crate::app::extensions_adapter::hooks;
use crate::app::extensions_adapter::plugin;
use crate::app::extensions_adapter::skill;
use crate::app::workflow_adapter::state;
use crate::app::workflow_adapter::transition;
use crate::composition::{config, dispatch, inference, uninstall};
use crate::evidence;
use crate::foundation::error::AppError;
use crate::intent;
use crate::ontology;
use crate::patch;
use crate::runtime;
use crate::surfaces::cli::{
    command::{
        Command, EvidenceCommand, HooksCommand, IntentCommand, MonitorCommand, OntologyCommand,
        PatchCommand, PluginCommand, PolicyCommand, PolicyPathMode, SessionCommand, SkillCommand,
        StateCommand, TuiCommand, UninstallCommand,
    },
    render,
};
use crate::tui;

use super::monitor_adapter as monitor;
use super::policy_adapter as policy;

mod collaboration_commands;
mod inference_ports;

use collaboration_commands::{execute_subagent, execute_team};
use inference_ports::emit_output as emit_inference_output;

pub(super) struct LegacyCommandDispatchPort;

impl dispatch::CommandDispatchPort for LegacyCommandDispatchPort {
    fn terminal_attached(&mut self) -> bool {
        capability::attached()
    }

    fn validate_native_terminal(&mut self) -> Result<(), AppError> {
        native::validate_native_fault_configuration().map_err(crate::tui::terminal_fault_error)
    }

    fn recover_pending_source_bundles(&mut self) -> Result<(), AppError> {
        transition::recover_pending_source_bundles().map(|_| ())
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
            Command::Team(command) => execute_team(command),
            Command::Subagent(command) => execute_subagent(command),
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
