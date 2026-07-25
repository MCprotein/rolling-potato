use crate::foundation::error::AppError;
use crate::runtime_core::terminal::{TerminalChoice, TerminalIo};

use super::super::runtime_bridge::{TuiSessionOption, TuiSessionTransition};
use super::terminal_flow::terminal_fault_error;
use super::TuiRuntimePort;
use crate::surfaces::tui::view_model::{InteractiveState, InteractiveView};

pub(super) fn start_new_session(runtime: &mut impl TuiRuntimePort, state: &mut InteractiveState) {
    match runtime.start_new_session() {
        Ok(transition) => apply_session_transition(state, transition),
        Err(error) => state.notice = error.message,
    }
}

pub(super) fn resume_session(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
) -> Result<(), AppError> {
    let options = runtime.session_options()?;
    if options.is_empty() {
        state.notice = "재개할 이전 세션이 없습니다.".to_string();
        return Ok(());
    }
    let Some(session_id) = choose_session(terminal, &options)? else {
        state.notice = "세션 재개를 취소했습니다.".to_string();
        return Ok(());
    };
    resume_selected_session(runtime, state, &session_id);
    Ok(())
}

pub(super) fn resume_selected_session(
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    session_id: &str,
) {
    match runtime.resume_session(session_id) {
        Ok(transition) => apply_session_transition(state, transition),
        Err(error) => state.notice = error.message,
    }
}

fn choose_session(
    terminal: &mut impl TerminalIo,
    options: &[TuiSessionOption],
) -> Result<Option<String>, AppError> {
    let choices = options
        .iter()
        .map(|option| TerminalChoice {
            value: option.session_id.clone(),
            label: short_session_id(&option.session_id),
            description: option.preview.clone(),
            current: option.current,
            recommended: false,
        })
        .collect::<Vec<_>>();
    terminal
        .choose("세션 재개", &choices)
        .map_err(terminal_fault_error)
}

fn apply_session_transition(state: &mut InteractiveState, transition: TuiSessionTransition) {
    state.view = InteractiveView::Conversation;
    state.page = 0;
    state.selected_id = None;
    state.turns = transition.turns;
    state.attachments.clear();
    state.notice_page = 0;
    state.notice = transition.notice;
}

fn short_session_id(session_id: &str) -> String {
    if session_id.chars().count() <= 24 {
        session_id.to_string()
    } else {
        format!("{}…", session_id.chars().take(23).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_session_ids_are_bounded_for_the_picker() {
        assert_eq!(
            short_session_id("session-123456789012345678901234567890"),
            "session-123456789012345…"
        );
    }
}
