//! CLI execution adapter for TUI entrypoints and reports.

use crate::adapters::filesystem::layout as paths;
use crate::adapters::terminal::capability;
use crate::app::tui_adapter as tui;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::TuiCommand;

pub(super) fn execute_tui(command: TuiCommand) -> Result<(), AppError> {
    match command {
        TuiCommand::Auto => {
            if cfg!(unix) && capability::attached() && !paths::current_state_file().is_file() {
                state::initialize()?;
            }
            tui::run_auto()
        }
        TuiCommand::Interactive => {
            if cfg!(unix) && !paths::current_state_file().is_file() {
                state::initialize()?;
            }
            tui::run_interactive()
        }
        TuiCommand::Monitor => print_report(tui::monitor_report()),
        TuiCommand::Sessions => print_report(tui::sessions_report()),
        TuiCommand::Transcript { session_id } => print_report(tui::transcript_report(&session_id)),
        TuiCommand::Approvals => print_report(tui::approvals_report()),
        TuiCommand::Diff { proposal_id } => print_report(tui::diff_report(&proposal_id)),
        TuiCommand::Evidence => print_report(tui::evidence_report()),
    }
}

fn print_report(report: Result<String, AppError>) -> Result<(), AppError> {
    println!("{}", report?);
    Ok(())
}
