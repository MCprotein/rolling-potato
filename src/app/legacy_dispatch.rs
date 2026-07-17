use crate::adapters::filesystem::layout as paths;
use crate::adapters::terminal::{capability, native};
use crate::app::workflow_adapter::state;
use crate::app::workflow_adapter::transition;
use crate::composition::{config, dispatch, inference, uninstall};
use crate::foundation::error::AppError;
use crate::intent;
use crate::patch;
use crate::runtime;
use crate::surfaces::cli::{
    command::{
        Command, IntentCommand, PatchCommand, SessionCommand, StateCommand, TuiCommand,
        UninstallCommand,
    },
    render,
};
use crate::tui;

mod collaboration_commands;
mod extension_commands;
mod inference_ports;
mod knowledge_commands;
mod observability_commands;
mod policy_commands;

use collaboration_commands::{execute_subagent, execute_team};
use extension_commands::{execute_hooks, execute_plugin, execute_skill};
use inference_ports::emit_output as emit_inference_output;
use knowledge_commands::{execute_evidence, execute_ontology};
use observability_commands::{execute_cache_status, execute_monitor};
use policy_commands::execute_policy;

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
            Command::Evidence(command) => execute_evidence(command),
            Command::Skill(command) => execute_skill(command),
            Command::Policy(command) => execute_policy(command),
            Command::Hooks(command) => execute_hooks(command),
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
            Command::CacheStatus => execute_cache_status(),
            Command::Monitor(command) => execute_monitor(command),
            Command::Ontology(command) => execute_ontology(command),
            Command::Benchmark(command) => {
                emit_inference_output(inference::run_benchmark(command, self)?);
                Ok(())
            }
            Command::Model(command) => {
                emit_inference_output(inference::run_model(command, self)?);
                Ok(())
            }
            Command::Plugin(command) => execute_plugin(command),
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
