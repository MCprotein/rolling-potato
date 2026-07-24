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

pub(crate) fn resolve_suggestion(
    suggestions: &[TerminalSuggestion],
    input: &str,
) -> Option<String> {
    let input = input.trim();
    input
        .parse::<usize>()
        .ok()
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| suggestions.get(index))
        .or_else(|| {
            suggestions
                .iter()
                .find(|suggestion| suggestion.command.split_whitespace().next() == Some(input))
        })
        .and_then(|suggestion| suggestion.command.split_whitespace().next())
        .map(str::to_string)
}

pub(crate) fn render_plain_suggestions(suggestions: &[TerminalSuggestion]) -> String {
    let mut output = "명령 미리보기\n".to_string();
    for (index, suggestion) in suggestions.iter().enumerate() {
        output.push_str(&format!(
            "{}. {}\n   {}\n",
            index + 1,
            suggestion.command,
            suggestion.description
        ));
    }
    output.push_str("번호 또는 명령어를 입력하세요 (빈 입력: 돌아가기): ");
    output
}

pub(crate) fn read_plain_suggestion<T: TerminalIo + ?Sized>(
    terminal: &mut T,
    suggestions: &[TerminalSuggestion],
) -> Result<Option<String>, TerminalFault> {
    let input = terminal.read_line()?;
    if input.as_deref().map(str::trim) != Some("/") {
        return Ok(input);
    }
    terminal.write_frame(&render_plain_suggestions(suggestions))?;
    terminal
        .read_line()
        .map(|input| input.map(|input| resolve_suggestion(suggestions, &input).unwrap_or(input)))
}

pub(crate) fn read_plain_choice<T: TerminalIo + ?Sized>(
    terminal: &mut T,
    title: &str,
    choices: &[TerminalChoice],
) -> Result<Option<String>, TerminalFault> {
    terminal.write_frame(&render_plain_choices(title, choices))?;
    terminal
        .read_line()
        .map(|input| input.and_then(|input| resolve_choice(choices, &input)))
}

pub(crate) trait TerminalIo {
    fn validate_configuration(&mut self) -> Result<(), TerminalFault> {
        Ok(())
    }

    fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault>;
    fn read_line(&mut self) -> Result<Option<String>, TerminalFault>;
    fn read_line_with_suggestions(
        &mut self,
        suggestions: &[TerminalSuggestion],
    ) -> Result<Option<String>, TerminalFault> {
        read_plain_suggestion(self, suggestions)
    }
    fn read_secret(&mut self) -> Result<Option<String>, TerminalFault>;
    fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault>;

    fn choose(
        &mut self,
        title: &str,
        choices: &[TerminalChoice],
    ) -> Result<Option<String>, TerminalFault> {
        read_plain_choice(self, title, choices)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    const SUGGESTIONS: &[TerminalSuggestion] = &[
        TerminalSuggestion {
            command: "/model [id]",
            description: "모델 확인 및 변경",
        },
        TerminalSuggestion {
            command: "/search <질문>",
            description: "인터넷 검색",
        },
    ];

    struct PlainTerminal {
        lines: VecDeque<Option<String>>,
        frames: Vec<String>,
    }

    impl PlainTerminal {
        fn new(lines: &[&str]) -> Self {
            Self {
                lines: lines.iter().map(|line| Some((*line).to_string())).collect(),
                frames: Vec::new(),
            }
        }
    }

    impl TerminalIo for PlainTerminal {
        fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault> {
            Ok((80, 24))
        }

        fn read_line(&mut self) -> Result<Option<String>, TerminalFault> {
            Ok(self.lines.pop_front().flatten())
        }

        fn read_secret(&mut self) -> Result<Option<String>, TerminalFault> {
            Ok(None)
        }

        fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault> {
            self.frames.push(frame.to_string());
            Ok(())
        }
    }

    #[test]
    fn plain_suggestions_render_commands_and_resolve_numbers_or_names() {
        let rendered = render_plain_suggestions(SUGGESTIONS);

        assert!(rendered.contains("1. /model [id]"));
        assert!(rendered.contains("2. /search <질문>"));
        assert_eq!(
            resolve_suggestion(SUGGESTIONS, "1").as_deref(),
            Some("/model")
        );
        assert_eq!(
            resolve_suggestion(SUGGESTIONS, "/search").as_deref(),
            Some("/search")
        );
        assert_eq!(resolve_suggestion(SUGGESTIONS, "99"), None);
    }

    #[test]
    fn plain_terminal_shows_palette_after_slash_and_resolves_number() {
        let mut terminal = PlainTerminal::new(&["/", "1"]);

        let selected = terminal.read_line_with_suggestions(SUGGESTIONS).unwrap();

        assert_eq!(selected.as_deref(), Some("/model"));
        assert_eq!(terminal.frames.len(), 1);
        assert!(terminal.frames[0].contains("명령 미리보기"));
        assert!(terminal.frames[0].contains("/search <질문>"));
    }

    #[test]
    fn plain_terminal_shows_choices_before_reading_selection() {
        let mut terminal = PlainTerminal::new(&["1"]);
        let choices = [TerminalChoice {
            value: "gemma".to_string(),
            label: "Gemma".to_string(),
            description: "로컬 모델".to_string(),
            current: true,
            recommended: false,
        }];

        let selected = terminal.choose("모델 선택", &choices).unwrap();

        assert_eq!(selected.as_deref(), Some("gemma"));
        assert!(terminal.frames[0].contains("모델 선택"));
        assert!(terminal.frames[0].contains("1. Gemma · 현재"));
    }
}
