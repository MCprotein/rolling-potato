//! CLI execution adapter for install, init, update, and uninstall lifecycle commands.

use crate::adapters::terminal::capability;
use crate::app::runtime_adapter as runtime;
use crate::composition::{install, uninstall, update};
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{InstallCommand, UninstallCommand, UpdateCommand};

pub(super) fn execute_install(command: InstallCommand) -> Result<(), AppError> {
    emit(&install::install_report(command)?)
}

pub(super) fn execute_init() -> Result<(), AppError> {
    let environment = install::init_environment_report()?;
    emit(&format!("{}\n\n{}", runtime::init_report()?, environment))?;
    if capability::attached() {
        crate::app::tui_adapter::run_setup()?;
    }
    Ok(())
}

pub(super) fn execute_update(command: UpdateCommand) -> Result<(), AppError> {
    let report = match command {
        UpdateCommand::Check => update::check_report()?,
        UpdateCommand::Apply => update::update_report()?,
    };
    emit(&report)
}

pub(super) fn execute_uninstall(command: UninstallCommand) -> Result<(), AppError> {
    emit(&uninstall::uninstall_report(command)?)
}

fn emit(report: &str) -> Result<(), AppError> {
    crate::surfaces::cli::render::emit_report(report);
    Ok(())
}
