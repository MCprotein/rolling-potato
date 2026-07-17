//! CLI execution adapter for workflow state, session, and patch commands.

use crate::app::patch_adapter as patch;
use crate::app::runtime_adapter as runtime;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{PatchCommand, SessionCommand, StateCommand};

pub(super) fn execute_state(command: StateCommand) -> Result<(), AppError> {
    let report = match command {
        StateCommand::Status => state::status_report()?,
        StateCommand::Reconcile => state::reconcile_report()?,
        StateCommand::Resume => runtime::workflow_resume_report()?,
    };
    println!("{report}");
    Ok(())
}

pub(super) fn execute_session(command: SessionCommand) -> Result<(), AppError> {
    let report = match command {
        SessionCommand::List => state::session_list_report()?,
        SessionCommand::New => state::session_new_report()?,
        SessionCommand::Resume { id } => runtime::session_resume_report(&id)?,
    };
    println!("{report}");
    Ok(())
}

pub(super) fn execute_patch(command: PatchCommand) -> Result<(), AppError> {
    match command {
        PatchCommand::Preview {
            path,
            find,
            replace,
        } => println!("{}", patch::preview_report(&path, &find, &replace)?),
        PatchCommand::Approve {
            proposal_id,
            token,
            dry_run,
        } => return runtime::patch_approve_to_stdout(&proposal_id, &token, dry_run, None),
        PatchCommand::Verify { proposal_id, token } => {
            println!("{}", runtime::patch_verify_report(&proposal_id, &token)?);
        }
        PatchCommand::TokenRotate { proposal_id } => {
            println!("{}", patch::rotate_workflow_token_report(&proposal_id)?);
        }
    }
    Ok(())
}
