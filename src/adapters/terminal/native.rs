use std::io::{self, IsTerminal, Write};

pub(crate) use crate::runtime_core::terminal::{
    render_plain_choices, resolve_choice, FrameWriteBoundary, TerminalChoice, TerminalFault,
    TerminalIo, TerminalSuggestion,
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
}

impl NativeTerminal {
    pub fn new() -> Self {
        Self {
            allow_piped_dimensions: false,
            last_frame: String::new(),
        }
    }

    pub fn explicit_line_mode() -> Self {
        Self {
            allow_piped_dimensions: true,
            last_frame: String::new(),
        }
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

    fn read_line_with_suggestions(
        &mut self,
        suggestions: &[TerminalSuggestion],
    ) -> Result<Option<String>, TerminalFault> {
        if io::stdin().is_terminal() && self.supports_ansi_layout() && self.supports_color() {
            platform::read_line_with_suggestions(suggestions, &self.last_frame)
        } else {
            self.read_line()
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
        if io::stdin().is_terminal() && self.supports_ansi_layout() && self.supports_color() {
            platform::choose(title, choices)
        } else {
            self.write_frame(&render_plain_choices(title, choices))?;
            self.read_line()
                .map(|input| input.and_then(|input| resolve_choice(choices, &input)))
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
pub struct ScriptedTerminal {
    dimensions: std::collections::VecDeque<Result<(u16, u16), TerminalFault>>,
    lines: std::collections::VecDeque<Result<Option<String>, TerminalFault>>,
    secrets: std::collections::VecDeque<Result<Option<String>, TerminalFault>>,
    pub frames: Vec<String>,
    pub frame_fault: Option<TerminalFault>,
}

#[cfg(test)]
impl ScriptedTerminal {
    pub fn new(lines: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            dimensions: std::iter::repeat_n(Ok((80, 24)), 64).collect(),
            lines: lines
                .into_iter()
                .map(|line| Ok(Some(line.to_string())))
                .chain(std::iter::once(Ok(None)))
                .collect(),
            secrets: std::collections::VecDeque::new(),
            frames: Vec::new(),
            frame_fault: None,
        }
    }
}

#[cfg(test)]
impl TerminalIo for ScriptedTerminal {
    fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault> {
        self.dimensions.pop_front().unwrap_or(Ok((80, 24)))
    }

    fn read_line(&mut self) -> Result<Option<String>, TerminalFault> {
        self.lines.pop_front().unwrap_or(Ok(None))
    }

    fn read_secret(&mut self) -> Result<Option<String>, TerminalFault> {
        self.secrets.pop_front().unwrap_or(Ok(None))
    }

    fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault> {
        if let Some(fault) = self.frame_fault.take() {
            return Err(fault);
        }
        self.frames.push(frame.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_terminal_handles_eof_and_records_frames() {
        let mut terminal = ScriptedTerminal::new(["help", "quit"]);
        assert_eq!(terminal.dimensions().unwrap(), (80, 24));
        assert_eq!(terminal.read_line().unwrap().as_deref(), Some("help"));
        terminal.write_frame("frame\n").unwrap();
        assert_eq!(terminal.read_line().unwrap().as_deref(), Some("quit"));
        assert_eq!(terminal.read_line().unwrap(), None);
        assert_eq!(terminal.frames, ["frame\n"]);
    }

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
