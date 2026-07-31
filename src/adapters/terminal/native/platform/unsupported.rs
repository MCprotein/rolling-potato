use super::super::{TerminalChoice, TerminalFault, TerminalSuggestion};

pub fn begin_request_cancel_capture() -> Result<(), TerminalFault> {
    Ok(())
}

pub fn request_cancelled() -> bool {
    false
}

pub fn end_request_cancel_capture() {}

pub fn dimensions() -> Result<(u16, u16), TerminalFault> {
    Err(TerminalFault::SizeRead)
}
pub fn read_secret() -> Result<Option<String>, TerminalFault> {
    Err(TerminalFault::ModeRead)
}
pub fn read_input_with_suggestions(
    _suggestions: &[TerminalSuggestion],
    _base_frame: &str,
    _state: Option<super::super::live_input::State>,
) -> Result<super::super::live_input::ReadOutcome, TerminalFault> {
    Err(TerminalFault::ModeRead)
}

pub fn choose(_title: &str, _choices: &[TerminalChoice]) -> Result<Option<String>, TerminalFault> {
    Err(TerminalFault::ModeRead)
}
