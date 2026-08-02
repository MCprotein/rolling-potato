use crate::foundation::error::AppError;
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::runtime_core::terminal::{FrameWriteBoundary, TerminalIo};

use super::outcome::TuiOutcome;
use super::runtime_bridge::{
    SelectionLease, TuiAttachment, TuiGateKind, TuiIntent, TuiModelOption, TuiReadPage,
    TuiReadRequest, TuiRequestProgressReporter, TuiSessionOption, TuiSessionTransition,
    TuiStatusSnapshot, TuiWebSourceOption,
};
use super::view_model::{InteractiveState, InteractiveView};

mod attachments;
mod command_dispatch;
mod model_selection;
mod request_submission;
mod session_selection;
mod source_selection;
mod terminal_flow;

use command_dispatch::{dispatch_line, LoopControl};
#[cfg(test)]
pub(crate) use terminal_flow::consume_outcome;
pub(crate) use terminal_flow::terminal_fault_error;
use terminal_flow::{
    post_dispatch_write_error, pre_dispatch_write_error, read_input_action, read_status_or_notice,
    InputAction,
};

pub(crate) trait TuiRuntimePort: Send {
    fn startup_update_notice(&mut self) -> Option<String>;
    fn reconcile_existing_backend(&mut self) -> Result<(), AppError>;
    fn clear_conversation_history(&mut self) -> Result<(), AppError>;
    fn apply_update(&mut self) -> Result<String, AppError>;
    fn read_tui_page(&mut self, request: TuiReadRequest) -> Result<TuiReadPage, AppError>;
    fn read_tui_status(&mut self) -> Result<TuiStatusSnapshot, AppError>;
    fn model_options(&mut self) -> Vec<TuiModelOption>;
    fn session_options(&mut self) -> Result<Vec<TuiSessionOption>, AppError>;
    fn web_source_options(&mut self) -> Vec<TuiWebSourceOption>;
    fn select_web_source(&mut self, source_id: &str) -> Result<String, AppError>;
    fn start_new_session(&mut self) -> Result<TuiSessionTransition, AppError>;
    fn resume_session(&mut self, session_id: &str) -> Result<TuiSessionTransition, AppError>;
    fn setup_model(&mut self, id: &str) -> Result<String, AppError>;
    fn doctor_report(&mut self) -> String;
    fn compact_context(&mut self) -> Result<String, AppError>;
    fn capture_attachment(&mut self, path: &str) -> Result<TuiAttachment, AppError>;
    fn request_progress_hint(&mut self, _request: &str) -> Option<String> {
        None
    }
    fn request_context_tokens_hint(
        &mut self,
        _request: &str,
        _attachments: &[TuiAttachment],
    ) -> Option<u32> {
        None
    }
    fn submit_request(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
    ) -> Result<String, AppError>;
    fn submit_request_with_progress(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
        _progress: &TuiRequestProgressReporter,
        _cancellation: &RequestCancellationToken,
    ) -> Result<String, AppError> {
        self.submit_request(request, attachments)
    }
    fn new_tui_intent_id(&mut self) -> String;
    fn tui_selection_lease(&mut self, selected_object_id: &str)
        -> Result<SelectionLease, AppError>;
    fn tui_gate_descriptor(&mut self, workflow_id: &str)
        -> Result<(String, TuiGateKind), AppError>;
    fn dispatch_tui_intent(&mut self, intent: TuiIntent) -> Result<TuiOutcome, AppError>;
}

pub(crate) fn run_controller(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
) -> Result<(), AppError> {
    terminal
        .validate_configuration()
        .map_err(terminal_fault_error)?;
    let mut state = InteractiveState::new();
    if let Err(error) = runtime.reconcile_existing_backend() {
        state.notice = error.message;
    }
    let mut startup_update_pending = true;
    let mut post_dispatch_intent: Option<String> = None;

    loop {
        let (width, height) = terminal.dimensions().map_err(terminal_fault_error)?;
        let page = if matches!(state.view, InteractiveView::Conversation) {
            TuiReadPage::conversation_placeholder()
        } else {
            let request = state.read_request(width, height);
            runtime.read_tui_page(request)?
        };
        let status = read_status_or_notice(runtime, &mut state);
        let frame = super::render::render_interactive_frame_with_options(
            &state,
            &page,
            &status,
            width,
            height,
            terminal.supports_ansi_layout(),
            terminal.supports_color(),
        );
        let boundary = if post_dispatch_intent.is_some() {
            FrameWriteBoundary::PostDispatch
        } else {
            FrameWriteBoundary::Ordinary
        };
        if terminal.write_frame_at(&frame, boundary).is_err() {
            return Err(match post_dispatch_intent.take() {
                Some(intent_id) => post_dispatch_write_error(&intent_id),
                None => pre_dispatch_write_error(&runtime.new_tui_intent_id()),
            });
        }
        post_dispatch_intent = None;

        if startup_update_pending {
            startup_update_pending = false;
            if let Some(notice) = runtime.startup_update_notice() {
                state.notice = notice;
                continue;
            }
        }

        let line = match read_input_action(terminal, &mut state, width, height)? {
            InputAction::Command(line) => line,
            InputAction::Redraw => continue,
            InputAction::End => return Ok(()),
        };
        match dispatch_line(terminal, runtime, &mut state, &page, width, height, &line)? {
            LoopControl::Continue => {}
            LoopControl::Exit => return Ok(()),
            LoopControl::PostDispatch(intent_id) => post_dispatch_intent = Some(intent_id),
        }
    }
}
