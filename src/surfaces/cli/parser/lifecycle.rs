use crate::foundation::error::AppError;
use crate::surfaces::cli::command::{
    Command, SessionCommand, StateCommand, TuiCommand, UpdateCommand,
};

pub(super) fn parse_update(args: &[String]) -> Result<UpdateCommand, AppError> {
    match args {
        [] => Ok(UpdateCommand::Apply),
        [flag] if flag == "--check" => Ok(UpdateCommand::Check),
        _ => Err(AppError::usage(
            "update는 옵션 없이 적용하거나 `rpotato update --check`로 확인할 수 있습니다.",
        )),
    }
}

pub(super) fn parse_state(args: &[String]) -> Result<StateCommand, AppError> {
    match args {
        [] => Ok(StateCommand::Status),
        [action] if action == "reconcile" => Ok(StateCommand::Reconcile),
        [action] if action == "resume" => Ok(StateCommand::Resume),
        _ => Err(AppError::usage(
            "state 명령은 status 생략형, reconcile, resume만 허용합니다.",
        )),
    }
}

pub(super) fn parse_resume(args: &[String]) -> Result<Command, AppError> {
    match args {
        [] => Ok(Command::Session(SessionCommand::List)),
        [id] => Ok(Command::Session(SessionCommand::Resume { id: id.clone() })),
        _ => Err(AppError::usage(
            "resume은 인자 없이 session history를 보거나 resume <session-id> 형식만 허용합니다.",
        )),
    }
}

pub(super) fn parse_continue(args: &[String]) -> Result<Command, AppError> {
    match args {
        [] => Ok(Command::State(StateCommand::Resume)),
        [id] => Ok(Command::Session(SessionCommand::Resume {
            id: id.clone(),
        })),
        _ => Err(AppError::usage(
            "continue는 인자 없이 현재 workflow를 이어가거나 continue <session-id> 형식만 허용합니다.",
        )),
    }
}

pub(super) fn parse_session(args: &[String]) -> Result<SessionCommand, AppError> {
    match args {
        [action] if action == "list" || action == "history" => Ok(SessionCommand::List),
        [action] if action == "new" => Ok(SessionCommand::New),
        [action, id] if action == "resume" => Ok(SessionCommand::Resume { id: id.clone() }),
        [action, ..] if action == "resume" => Err(AppError::usage(
            "session resume에는 session id가 필요합니다.",
        )),
        _ => Err(AppError::usage(
            "session 명령은 list, history, new, resume만 허용합니다.",
        )),
    }
}

pub(super) fn parse_tui(args: &[String]) -> Result<TuiCommand, AppError> {
    match args {
        [] => Ok(TuiCommand::Auto),
        [action] if action == "interactive" => Ok(TuiCommand::Interactive),
        [action] if action == "monitor" => Ok(TuiCommand::Monitor),
        [action] if action == "sessions" => Ok(TuiCommand::Sessions),
        [action, session_id] if action == "transcript" => Ok(TuiCommand::Transcript {
            session_id: session_id.clone(),
        }),
        [action, ..] if action == "transcript" => Err(AppError::usage(
            "tui transcript에는 session id가 필요합니다.",
        )),
        [action] if action == "approvals" => Ok(TuiCommand::Approvals),
        [action, proposal_id] if action == "diff" => Ok(TuiCommand::Diff {
            proposal_id: proposal_id.clone(),
        }),
        [action, ..] if action == "diff" => {
            Err(AppError::usage("tui diff에는 proposal id가 필요합니다."))
        }
        [action] if action == "evidence" => Ok(TuiCommand::Evidence),
        _ => Err(AppError::usage(
            "tui 명령은 인자 없음, interactive, monitor, sessions, transcript, approvals, diff, evidence만 허용합니다.",
        )),
    }
}
