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

pub(crate) trait TerminalIo {
    fn validate_configuration(&mut self) -> Result<(), TerminalFault> {
        Ok(())
    }

    fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault>;
    fn read_line(&mut self) -> Result<Option<String>, TerminalFault>;
    fn read_secret(&mut self) -> Result<Option<String>, TerminalFault>;
    fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault>;

    fn write_frame_at(
        &mut self,
        frame: &str,
        _boundary: FrameWriteBoundary,
    ) -> Result<(), TerminalFault> {
        self.write_frame(frame)
    }
}
