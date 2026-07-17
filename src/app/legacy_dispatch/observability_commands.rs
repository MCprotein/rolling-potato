//! CLI execution adapter for cache and monitor commands.

use crate::adapters::filesystem::cache;
use crate::app::monitor_adapter as monitor;
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::MonitorCommand;

pub(super) fn execute_cache_status() -> Result<(), AppError> {
    println!("{}", cache::status_report());
    Ok(())
}

pub(super) fn execute_monitor(command: MonitorCommand) -> Result<(), AppError> {
    match command {
        MonitorCommand::Status => println!("{}", monitor::status_report()?),
        MonitorCommand::Models => println!("{}", monitor::models_report()?),
        MonitorCommand::Baseline => println!("{}", monitor::baseline_report()?),
        MonitorCommand::Optimize => println!("{}", monitor::optimize_report()?),
        MonitorCommand::Export { format } => print!("{}", monitor::export_report(format)?),
        MonitorCommand::Prune {
            before_days,
            dry_run,
        } => println!("{}", monitor::prune_report(before_days, dry_run)?),
    }
    Ok(())
}
