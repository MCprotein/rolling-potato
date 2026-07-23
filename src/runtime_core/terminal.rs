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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalChoice {
    pub(crate) value: String,
    pub(crate) label: String,
    pub(crate) description: String,
    pub(crate) current: bool,
    pub(crate) recommended: bool,
}

pub(crate) fn resolve_choice(choices: &[TerminalChoice], input: &str) -> Option<String> {
    let input = input.trim();
    input
        .parse::<usize>()
        .ok()
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| choices.get(index))
        .or_else(|| choices.iter().find(|choice| choice.value == input))
        .map(|choice| choice.value.clone())
}

pub(crate) fn render_plain_choices(title: &str, choices: &[TerminalChoice]) -> String {
    let mut output = format!("{title}\n");
    for (index, choice) in choices.iter().enumerate() {
        let current = if choice.current { " · 현재" } else { "" };
        let recommended = if choice.recommended { " · 권장" } else { "" };
        output.push_str(&format!(
            "{}. {}{current}{recommended}\n   {}\n",
            index + 1,
            choice.label,
            choice.description
        ));
    }
    output.push_str("번호 또는 id를 입력하세요 (빈 입력: 취소): ");
    output
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

    fn choose(
        &mut self,
        title: &str,
        choices: &[TerminalChoice],
    ) -> Result<Option<String>, TerminalFault> {
        self.write_frame(&render_plain_choices(title, choices))?;
        let Some(input) = self.read_line()? else {
            return Ok(None);
        };
        Ok(resolve_choice(choices, &input))
    }

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
