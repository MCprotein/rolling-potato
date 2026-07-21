//! Concrete CLI command dispatch adapter.

use crate::adapters::terminal::{capability, native};
use crate::app::intent_adapter as intent;
use crate::app::runtime_adapter as runtime;
use crate::app::workflow_adapter::state;
use crate::app::workflow_adapter::transition;
use crate::composition::{config, dispatch, inference, install, uninstall, update};
use crate::foundation::error::AppError;
use crate::surfaces::cli::{
    command::{Command, IntentCommand, UpdateCommand},
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

pub(super) struct CommandDispatchAdapter;

impl dispatch::CommandDispatchPort for CommandDispatchAdapter {
    fn terminal_attached(&mut self) -> bool {
        capability::attached()
    }

    fn validate_native_terminal(&mut self) -> Result<(), AppError> {
        native::validate_native_fault_configuration()
            .map_err(crate::app::tui_adapter::terminal_fault_error)
    }

    fn recover_pending_source_bundles(&mut self) -> Result<(), AppError> {
        transition::recover_pending_source_bundles().map(|_| ())
    }

    fn execute(&mut self, command: Command) -> Result<(), AppError> {
        match command {
            Command::Help => {
                render::emit_report(render::HELP);
                Ok(())
            }
            Command::AdvancedHelp => {
                render::emit_report(render::ADVANCED_HELP);
                Ok(())
            }
            Command::Install(command) => {
                render::emit_report(&install::install_report(command)?);
                Ok(())
            }
            Command::Update(command) => {
                let report = match command {
                    UpdateCommand::Check => update::check_report()?,
                    UpdateCommand::Apply => update::update_report()?,
                };
                render::emit_report(&report);
                Ok(())
            }
            Command::Init => {
                let environment = install::init_environment_report()?;
                render::emit_report(&format!("{}\n\n{}", runtime::init_report()?, environment));
                if capability::attached() {
                    crate::app::tui_adapter::run_setup()?;
                }
                Ok(())
            }
            Command::Run { request } => {
                render::emit_report(&runtime::agent_run_report(&request)?);
                Ok(())
            }
            Command::Intent(IntentCommand::Classify { request }) => {
                render::emit_report(&intent::classify_report(&request)?);
                Ok(())
            }
            Command::Intent(IntentCommand::Routes) => {
                render::emit_report(&intent::routes_report());
                Ok(())
            }
            Command::Doctor => {
                render::emit_report(&runtime::doctor_report());
                Ok(())
            }
            Command::Config => {
                render::emit_report(&config::report());
                Ok(())
            }
            Command::State(command) => execute_state(command),
            Command::Session(command) => execute_session(command),
            Command::Team(command) => execute_team(command),
            Command::Subagent(command) => execute_subagent(command),
            Command::Tui(command) => execute_tui(command),
            Command::Cancel => {
                render::emit_report(&state::cancel_report()?);
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
            Command::Uninstall(command) => {
                render::emit_report(&uninstall::uninstall_report(command)?);
                Ok(())
            }
        }
    }
}
