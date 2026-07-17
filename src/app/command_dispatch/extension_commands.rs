//! CLI execution adapter for skill, hook, and plugin commands.

use crate::app::extensions_adapter::{hooks, plugin, skill};
use crate::app::intent_adapter as intent;
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{HooksCommand, PluginCommand, SkillCommand};

pub(super) fn execute_skill(command: SkillCommand) -> Result<(), AppError> {
    let report = match command {
        SkillCommand::List => skill::list_report(),
        SkillCommand::Run { id, request } => intent::run_skill_report(&id, &request)?,
    };
    println!("{report}");
    Ok(())
}

pub(super) fn execute_hooks(command: HooksCommand) -> Result<(), AppError> {
    let report = match command {
        HooksCommand::List => hooks::list_report(),
        HooksCommand::ValidateResult { json } => hooks::validate_result_report(&json)?,
    };
    println!("{report}");
    Ok(())
}

pub(super) fn execute_plugin(command: PluginCommand) -> Result<(), AppError> {
    let report = match command {
        PluginCommand::Import {
            source,
            path,
            dry_run,
        } => plugin::import_report(source, &path, dry_run)?,
        PluginCommand::List => plugin::list_report(),
        PluginCommand::Inspect { id } => plugin::inspect_report(&id)?,
        PluginCommand::Validate { id } => plugin::validate_report(&id)?,
        PluginCommand::Enable { id } => plugin::set_enabled_report(&id, true)?,
        PluginCommand::Disable { id } => plugin::set_enabled_report(&id, false)?,
        PluginCommand::Remove { id, purge_data } => plugin::remove_report(&id, purge_data)?,
    };
    println!("{report}");
    Ok(())
}
