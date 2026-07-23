use std::io::{self, Read, Write};

use crate::runtime_core::terminal::{TerminalChoice, TerminalFault};

pub(in crate::adapters::terminal::native) fn choose(
    title: &str,
    choices: &[TerminalChoice],
    terminal_width: usize,
) -> Result<Option<String>, TerminalFault> {
    if choices.is_empty() {
        return Ok(None);
    }
    let mut selected = choices
        .iter()
        .position(|choice| choice.current)
        .unwrap_or(0);
    let mut escape = Vec::new();
    let mut stdin = io::stdin().lock();
    redraw(title, choices, selected, terminal_width)?;
    loop {
        let mut byte = [0_u8; 1];
        let bytes = stdin.read(&mut byte).map_err(|_| TerminalFault::LineRead)?;
        if bytes == 0 {
            if escape == [0x1b] {
                return Ok(None);
            }
            continue;
        }
        let byte = byte[0];
        if !escape.is_empty() || byte == 0x1b {
            escape.push(byte);
            if escape_complete(&escape) {
                match escape.as_slice() {
                    b"\x1b[A" | b"\x1bOA" => {
                        selected = selected.checked_sub(1).unwrap_or(choices.len() - 1);
                    }
                    b"\x1b[B" | b"\x1bOB" => selected = (selected + 1) % choices.len(),
                    b"\x1b[H" | b"\x1bOH" => selected = 0,
                    b"\x1b[F" | b"\x1bOF" => selected = choices.len() - 1,
                    _ => {}
                }
                escape.clear();
                redraw(title, choices, selected, terminal_width)?;
            }
            continue;
        }
        match byte {
            b'\n' | b'\r' => return Ok(Some(choices[selected].value.clone())),
            0x03 => return Ok(None),
            0x10 => selected = selected.checked_sub(1).unwrap_or(choices.len() - 1),
            0x0e => selected = (selected + 1) % choices.len(),
            b'1'..=b'9' => {
                let index = usize::from(byte - b'1');
                if let Some(choice) = choices.get(index) {
                    return Ok(Some(choice.value.clone()));
                }
            }
            _ => continue,
        }
        redraw(title, choices, selected, terminal_width)?;
    }
}

fn escape_complete(sequence: &[u8]) -> bool {
    sequence.len() >= 3
        && matches!(sequence[1], b'[' | b'O')
        && matches!(sequence.last(), Some(0x40..=0x7e))
        || sequence.len() >= 16
}

fn redraw(
    title: &str,
    choices: &[TerminalChoice],
    selected: usize,
    terminal_width: usize,
) -> Result<(), TerminalFault> {
    let width = terminal_width.clamp(40, 120);
    let mut output = format!("\u{001b}[2J\u{001b}[H\u{001b}[1;36m{title}\u{001b}[0m\n\n");
    for (index, choice) in choices.iter().enumerate() {
        let marker = if index == selected { "›" } else { " " };
        let current = if choice.current { "  현재" } else { "" };
        let recommended = if choice.recommended { "  권장" } else { "" };
        let label = truncate(
            &format!(
                "{marker} {}. {}{current}{recommended}",
                index + 1,
                choice.label
            ),
            width,
        );
        if index == selected {
            output.push_str(&format!("\u{001b}[1;36m{label}\u{001b}[0m\n"));
        } else {
            output.push_str(&format!("{label}\n"));
        }
        output.push_str(&format!(
            "   \u{001b}[2m{}\u{001b}[0m\n\n",
            truncate(&choice.description, width.saturating_sub(3))
        ));
    }
    output.push_str("\u{001b}[2m↑↓ 이동  Enter 선택  Esc 닫기  숫자 빠른 선택\u{001b}[0m\n");
    write_stdout(&output)
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

fn write_stdout(value: &str) -> Result<(), TerminalFault> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(value.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|_| TerminalFault::FrameWrite)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_escape_sequences_have_bounded_completion() {
        assert!(escape_complete(b"\x1b[A"));
        assert!(escape_complete(b"\x1bOF"));
        assert!(!escape_complete(b"\x1b["));
    }

    #[test]
    fn picker_labels_are_bounded() {
        assert_eq!(truncate("abcdefgh", 5), "abcd…");
        assert_eq!(truncate("한글", 5), "한글");
    }
}
