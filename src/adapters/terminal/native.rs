use std::io::{self, IsTerminal, Write};

#[cfg(windows)]
pub(crate) use crate::runtime_core::terminal::resolve_choice;
pub(crate) use crate::runtime_core::terminal::{
    read_plain_choice, read_plain_suggestion, FrameWriteBoundary, TerminalChoice, TerminalFault,
    TerminalInputEvent, TerminalIo, TerminalSuggestion,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestTerminalFault {
    SizeRead,
    ModeRead,
    NoEchoSet,
    SecretRead,
    FrameWriteBeforeDispatch,
    FrameWriteAfterDispatch,
}
pub fn validate_native_fault_configuration() -> Result<(), TerminalFault> {
    validate_test_fault_configuration()
}
pub struct NativeTerminal {
    allow_piped_dimensions: bool,
    last_frame: String,
    live_input_state: Option<live_input::State>,
}

impl NativeTerminal {
    pub fn new() -> Self {
        Self {
            allow_piped_dimensions: false,
            last_frame: String::new(),
            live_input_state: None,
        }
    }

    pub fn explicit_line_mode() -> Self {
        Self {
            allow_piped_dimensions: true,
            last_frame: String::new(),
            live_input_state: None,
        }
    }

    fn supports_live_input(&self) -> bool {
        platform::LIVE_INPUT
            && io::stdin().is_terminal()
            && self.supports_ansi_layout()
            && self.supports_color()
    }
}

impl TerminalIo for NativeTerminal {
    fn validate_configuration(&mut self) -> Result<(), TerminalFault> {
        validate_test_fault_configuration()
    }

    fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault> {
        inject_test_fault(TestTerminalFault::SizeRead, TerminalFault::SizeRead)?;
        match platform::dimensions() {
            Ok(size) => Ok(size),
            Err(_) if self.allow_piped_dimensions && !io::stdout().is_terminal() => {
                let columns = std::env::var("COLUMNS")
                    .ok()
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(80);
                let lines = std::env::var("LINES")
                    .ok()
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(24);
                Ok((columns, lines))
            }
            Err(fault) => Err(fault),
        }
    }

    fn read_line(&mut self) -> Result<Option<String>, TerminalFault> {
        read_stdin_line(TerminalFault::LineRead)
    }

    fn read_input_with_suggestions(
        &mut self,
        suggestions: &[TerminalSuggestion],
    ) -> Result<TerminalInputEvent, TerminalFault> {
        if self.supports_live_input() {
            let outcome = platform::read_input_with_suggestions(
                suggestions,
                &self.last_frame,
                self.live_input_state.take(),
            )?;
            self.live_input_state = outcome.state;
            Ok(outcome.event)
        } else {
            read_plain_suggestion(self, suggestions)
                .map(|line| line.map_or(TerminalInputEvent::End, TerminalInputEvent::Submit))
        }
    }

    fn read_secret(&mut self) -> Result<Option<String>, TerminalFault> {
        platform::read_secret()
    }

    fn choose(
        &mut self,
        title: &str,
        choices: &[TerminalChoice],
    ) -> Result<Option<String>, TerminalFault> {
        if self.supports_live_input() {
            platform::choose(title, choices)
        } else {
            read_plain_choice(self, title, choices)
        }
    }

    fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault> {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(frame.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|_| TerminalFault::FrameWrite)?;
        self.last_frame.clear();
        self.last_frame.push_str(frame);
        Ok(())
    }

    fn supports_ansi_layout(&self) -> bool {
        io::stdout().is_terminal()
            && std::env::var_os("TERM").as_deref() != Some(std::ffi::OsStr::new("dumb"))
    }

    fn supports_color(&self) -> bool {
        self.supports_ansi_layout() && std::env::var_os("NO_COLOR").is_none()
    }

    fn write_frame_at(
        &mut self,
        frame: &str,
        boundary: FrameWriteBoundary,
    ) -> Result<(), TerminalFault> {
        match boundary {
            FrameWriteBoundary::Ordinary => {}
            FrameWriteBoundary::PreDispatch => inject_test_fault(
                TestTerminalFault::FrameWriteBeforeDispatch,
                TerminalFault::FrameWrite,
            )?,
            FrameWriteBoundary::PostDispatch => inject_test_fault(
                TestTerminalFault::FrameWriteAfterDispatch,
                TerminalFault::FrameWrite,
            )?,
        }
        self.write_frame(frame)
    }
}

#[cfg(debug_assertions)]
fn configured_test_fault() -> Result<Option<TestTerminalFault>, TerminalFault> {
    parse_test_fault_value(std::env::var_os("RPOTATO_TEST_TERMINAL_FAULT").as_deref())
}

#[cfg(debug_assertions)]
fn parse_test_fault_value(
    value: Option<&std::ffi::OsStr>,
) -> Result<Option<TestTerminalFault>, TerminalFault> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    match value.to_str() {
        Some("size-read") => Ok(Some(TestTerminalFault::SizeRead)),
        Some("mode-read") => Ok(Some(TestTerminalFault::ModeRead)),
        Some("no-echo-set") => Ok(Some(TestTerminalFault::NoEchoSet)),
        Some("secret-read") => Ok(Some(TestTerminalFault::SecretRead)),
        Some("frame-write-before-dispatch") => {
            Ok(Some(TestTerminalFault::FrameWriteBeforeDispatch))
        }
        Some("frame-write-after-dispatch") => Ok(Some(TestTerminalFault::FrameWriteAfterDispatch)),
        _ => Err(TerminalFault::InvalidFaultConfiguration),
    }
}

#[cfg(debug_assertions)]
fn validate_test_fault_configuration() -> Result<(), TerminalFault> {
    configured_test_fault().map(|_| ())
}

#[cfg(not(debug_assertions))]
#[inline(always)]
fn validate_test_fault_configuration() -> Result<(), TerminalFault> {
    Ok(())
}

#[cfg(debug_assertions)]
fn inject_test_fault(
    expected: TestTerminalFault,
    fault: TerminalFault,
) -> Result<(), TerminalFault> {
    if configured_test_fault()? == Some(expected) {
        return Err(fault);
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
#[inline(always)]
fn inject_test_fault(
    _expected: TestTerminalFault,
    _fault: TerminalFault,
) -> Result<(), TerminalFault> {
    Ok(())
}

fn read_stdin_line(fault: TerminalFault) -> Result<Option<String>, TerminalFault> {
    let mut line = String::new();
    let bytes = io::stdin().read_line(&mut line).map_err(|_| fault)?;
    if bytes == 0 {
        return Ok(None);
    }
    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    Ok(Some(line))
}

fn zeroize_string(value: String) {
    let mut bytes = value.into_bytes();
    for byte in &mut bytes {
        // SAFETY: `byte` is a valid, uniquely borrowed byte in the owned buffer.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

mod live_input;
mod platform;
#[cfg(test)]
pub use crate::test_support::ScriptedTerminal;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(debug_assertions)]
    #[test]
    fn test_fault_configuration_has_an_exact_closed_value_set() {
        for (value, expected) in [
            ("size-read", TestTerminalFault::SizeRead),
            ("mode-read", TestTerminalFault::ModeRead),
            ("no-echo-set", TestTerminalFault::NoEchoSet),
            ("secret-read", TestTerminalFault::SecretRead),
            (
                "frame-write-before-dispatch",
                TestTerminalFault::FrameWriteBeforeDispatch,
            ),
            (
                "frame-write-after-dispatch",
                TestTerminalFault::FrameWriteAfterDispatch,
            ),
        ] {
            assert_eq!(
                parse_test_fault_value(Some(std::ffi::OsStr::new(value))).unwrap(),
                Some(expected)
            );
        }
        assert_eq!(parse_test_fault_value(None).unwrap(), None);
        assert_eq!(
            parse_test_fault_value(Some(std::ffi::OsStr::new(""))).unwrap(),
            None
        );
        assert_eq!(
            parse_test_fault_value(Some(std::ffi::OsStr::new("unknown"))).unwrap_err(),
            TerminalFault::InvalidFaultConfiguration
        );
    }
}
