//! CLI execution adapter for fail-closed policy commands.

use crate::app::policy_adapter as policy;
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{PolicyCommand, PolicyPathMode};

pub(super) fn execute_policy(command: PolicyCommand) -> Result<(), AppError> {
    let report = match command {
        PolicyCommand::Schema => policy::schema_report(),
        PolicyCommand::CheckCommand { command } => policy::check_command_report(&command)?,
        PolicyCommand::CheckPath { mode, path } => {
            let mode = match mode {
                PolicyPathMode::Read => policy::PathMode::Read,
                PolicyPathMode::Write => policy::PathMode::Write,
            };
            policy::check_path_report(mode, &path)?
        }
        PolicyCommand::Redact { text } => policy::redact_report(&text),
    };
    println!("{report}");
    Ok(())
}
