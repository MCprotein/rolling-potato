#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalFault {
    #[cfg(debug_assertions)]
    InvalidFaultConfiguration,
    SizeRead,
    ModeRead,
    NoEchoSet,
    LineRead,
    SecretRead,
    EchoRestore,
    FrameWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameWriteBoundary {
    Ordinary,
    PreDispatch,
    PostDispatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalSuggestion {
    pub(crate) command: &'static str,
    pub(crate) description: &'static str,
}

pub(crate) trait TerminalIo {
    fn validate_configuration(&mut self) -> Result<(), TerminalFault> {
        Ok(())
    }

    fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault>;
    fn read_line(&mut self) -> Result<Option<String>, TerminalFault>;
    fn read_line_with_suggestions(
        &mut self,
        _suggestions: &[TerminalSuggestion],
    ) -> Result<Option<String>, TerminalFault> {
        self.read_line()
    }
    fn read_secret(&mut self) -> Result<Option<String>, TerminalFault>;
    fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault>;

    fn supports_ansi_layout(&self) -> bool {
        false
    }

    fn supports_color(&self) -> bool {
        false
    }

    fn write_frame_at(
        &mut self,
        frame: &str,
        _boundary: FrameWriteBoundary,
    ) -> Result<(), TerminalFault> {
        self.write_frame(frame)
    }
}
