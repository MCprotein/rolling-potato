use crate::adapters::terminal::{capability, native};
use crate::app::workflow_adapter::state;
use crate::app::workflow_adapter::transition;
use crate::composition::{config, dispatch, inference, uninstall};
use crate::foundation::error::AppError;
use crate::intent;
use crate::runtime;
use crate::surfaces::cli::{
    command::{Command, IntentCommand, UninstallCommand},
    render,
};
mod collaboration_commands;
mod extension_commands;
mod inference_ports;
mod knowledge_commands;
mod observability_commands;
mod policy_commands;
mod tui_commands;
mod workflow_commands;

use collaboration_commands::{execute_subagent, execute_team};
use extension_commands::{execute_hooks, execute_plugin, execute_skill};
use inference_ports::emit_output as emit_inference_output;
use knowledge_commands::{execute_evidence, execute_ontology};
use observability_commands::{execute_cache_status, execute_monitor};
use policy_commands::execute_policy;
use tui_commands::execute_tui;
use workflow_commands::{execute_patch, execute_session, execute_state};

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
            Command::State(command) => execute_state(command),
            Command::Session(command) => execute_session(command),
            Command::Team(command) => execute_team(command),
            Command::Subagent(command) => execute_subagent(command),
            Command::Tui(command) => execute_tui(command),
            Command::Cancel => {
                println!("{}", state::cancel_report()?);
                Ok(())
            }
            Command::Evidence(command) => execute_evidence(command),
            Command::Skill(command) => execute_skill(command),
            Command::Policy(command) => execute_policy(command),
            Command::Hooks(command) => execute_hooks(command),
            Command::Patch(command) => execute_patch(command),
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
