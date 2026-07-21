//! CLI execution adapter for team and subagent commands.

use crate::app::collaboration_adapter::{
    subagent, team, team_execution, team_reconciliation, team_state,
};
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{SubagentCommand, TeamCommand};

pub(super) fn execute_team(command: TeamCommand) -> Result<(), AppError> {
    let report = match command {
        TeamCommand::Status => team::status_report()?,
        TeamCommand::Plan { manifest_path } => team_state::plan_report(&manifest_path)?,
        TeamCommand::Execute { team_id } => team_execution::execute_report(&team_id)?,
        TeamCommand::Reconcile { team_id } => team_reconciliation::reconcile_report(&team_id)?,
        TeamCommand::Cancel { team_id } => team_state::cancel_report(&team_id)?,
        TeamCommand::Admit {
            lanes,
            write_paths,
            owned_write_paths,
            commands,
        } => team::admission_report(lanes, &write_paths, &owned_write_paths, &commands)?,
        TeamCommand::Dispatch {
            lanes,
            owned_write_paths,
            failed_lane,
            failure_reason,
        } => team::dispatch_report(
            lanes,
            &owned_write_paths,
            failed_lane,
            failure_reason.as_deref(),
        )?,
        TeamCommand::Governor {
            lanes,
            context_tokens,
            context_limit,
            model_tier,
        } => team::governor_report(lanes, context_tokens, context_limit, model_tier)?,
    };
    crate::surfaces::cli::render::emit_report(&report);
    Ok(())
}

pub(super) fn execute_subagent(command: SubagentCommand) -> Result<(), AppError> {
    let report = match command {
        SubagentCommand::Launch {
            role,
            task,
            tools,
            read_paths,
            write_paths,
            timeout_ms,
            max_tokens,
        } => subagent::launch_report(
            &role,
            &task,
            &tools,
            &read_paths,
            &write_paths,
            timeout_ms,
            max_tokens,
        )?,
        SubagentCommand::Status { id } => subagent::status_report(id.as_deref())?,
        SubagentCommand::Cancel { id } => subagent::cancel_report(&id)?,
    };
    crate::surfaces::cli::render::emit_report(&report);
    Ok(())
}
