use std::io::{self, Read, Write};

use super::{TerminalFault, TerminalSuggestion};

const MAX_INPUT_BYTES: usize = 8 * 1024;
const MAX_PALETTE_ROWS: usize = 6;

pub(super) fn read(
    suggestions: &[TerminalSuggestion],
    terminal_width: usize,
) -> Result<Option<String>, TerminalFault> {
    let mut input = Vec::new();
    let mut rendered_rows = 0;
    let mut escape_bytes_to_skip = 0;
    let mut stdin = io::stdin().lock();
    redraw(&input, suggestions, terminal_width, &mut rendered_rows)?;

    loop {
        let mut byte = [0_u8; 1];
        let bytes = stdin.read(&mut byte).map_err(|_| TerminalFault::LineRead)?;
        if bytes == 0 || (byte[0] == 0x04 && input.is_empty()) {
            clear_palette(rendered_rows)?;
            return Ok(None);
        }
        if escape_bytes_to_skip > 0 {
            escape_bytes_to_skip -= 1;
            continue;
        }
        match byte[0] {
            b'\n' | b'\r' => {
                clear_palette(rendered_rows)?;
                return String::from_utf8(input)
                    .map(Some)
                    .map_err(|_| TerminalFault::LineRead);
            }
            0x08 | 0x7f => pop_last_utf8_char(&mut input),
            0x1b => {
                escape_bytes_to_skip = 2;
                continue;
            }
            byte if !byte.is_ascii_control() && input.len() < MAX_INPUT_BYTES => input.push(byte),
            _ => continue,
        }
        redraw(&input, suggestions, terminal_width, &mut rendered_rows)?;
    }
}

fn pop_last_utf8_char(input: &mut Vec<u8>) {
    let Some(last) = input.pop() else {
        return;
    };
    if last & 0b1100_0000 == 0b1000_0000 {
        while matches!(input.last(), Some(byte) if byte & 0b1100_0000 == 0b1000_0000) {
            input.pop();
        }
        input.pop();
    }
}

fn matching_suggestions<'a>(
    input: &str,
    suggestions: &'a [TerminalSuggestion],
) -> Vec<&'a TerminalSuggestion> {
    if !input.starts_with('/') || input.chars().any(char::is_whitespace) {
        return Vec::new();
    }
    suggestions
        .iter()
        .filter(|entry| {
            entry
                .command
                .split_whitespace()
                .next()
                .is_some_and(|command| command.starts_with(input))
        })
        .take(MAX_PALETTE_ROWS)
        .collect()
}

fn redraw(
    input: &[u8],
    suggestions: &[TerminalSuggestion],
    terminal_width: usize,
    rendered_rows: &mut usize,
) -> Result<(), TerminalFault> {
    let input = std::str::from_utf8(input).unwrap_or("");
    let matches = matching_suggestions(input, suggestions);
    let rows_to_clear = (*rendered_rows).max(matches.len());
    let input_width = terminal_width.saturating_sub(6).max(1);
    let visible_input = tail_by_cell_width(input, input_width);
    let cursor_column = 4 + display_cell_width(visible_input);
    let command_width = suggestions
        .iter()
        .map(|item| display_cell_width(item.command))
        .max()
        .unwrap_or(0);
    let mut output = String::new();

    output.push_str("\r\u{001b}[4C");
    output.push_str(visible_input);
    output.push_str(&" ".repeat(input_width.saturating_sub(display_cell_width(visible_input))));
    output.push_str("\u{001b}[2m│\u{001b}[0m");
    output.push('\r');
    output.push_str(&format!("\u{001b}[{}C\u{001b}7", cursor_column));

    for row in 0..rows_to_clear {
        output.push_str("\u{001b}8");
        output.push_str(&format!(
            "\u{001b}[{}A\r\u{001b}[2K",
            2 + rows_to_clear - row - 1
        ));
        let first_match_row = rows_to_clear.saturating_sub(matches.len());
        if row >= first_match_row {
            let entry = matches[row - first_match_row];
            let command_padding = command_width.saturating_sub(display_cell_width(entry.command));
            let description = truncate_plain_text(
                entry.description,
                terminal_width.saturating_sub(2 + command_width + 2),
            );
            output.push_str(&format!(
                "  \u{001b}[1;36m{}\u{001b}[0m{}  \u{001b}[2m{}\u{001b}[0m",
                entry.command,
                " ".repeat(command_padding),
                description
            ));
        }
    }
    output.push_str("\u{001b}8");

    write_stdout(&output)?;
    *rendered_rows = matches.len();
    Ok(())
}

fn clear_palette(rows: usize) -> Result<(), TerminalFault> {
    if rows == 0 {
        return Ok(());
    }
    let mut output = String::from("\u{001b}7");
    for distance in 2..(2 + rows) {
        output.push_str("\u{001b}8");
        output.push_str(&format!("\u{001b}[{distance}A\r\u{001b}[2K"));
    }
    output.push_str("\u{001b}8");
    write_stdout(&output)
}

fn write_stdout(value: &str) -> Result<(), TerminalFault> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(value.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|_| TerminalFault::FrameWrite)
}

fn tail_by_cell_width(value: &str, width: usize) -> &str {
    if display_cell_width(value) <= width {
        return value;
    }
    let mut used = 0;
    let mut start = value.len();
    for (index, ch) in value.char_indices().rev() {
        let next = used + terminal_cell_width(ch);
        if next > width {
            break;
        }
        used = next;
        start = index;
    }
    &value[start..]
}

fn truncate_plain_text(value: &str, width: usize) -> String {
    if display_cell_width(value) <= width {
        return value.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let available = width.saturating_sub(1);
    let mut used = 0;
    let mut output = String::new();
    for ch in value.chars() {
        let next = used + terminal_cell_width(ch);
        if next > available {
            break;
        }
        used = next;
        output.push(ch);
    }
    output.push('…');
    output
}

fn display_cell_width(value: &str) -> usize {
    value.chars().map(terminal_cell_width).sum()
}

fn terminal_cell_width(ch: char) -> usize {
    let code = ch as u32;
    if ch.is_control()
        || ch == '\u{200d}'
        || matches!(
            code,
            0x0300..=0x036f
                | 0x1ab0..=0x1aff
                | 0x1dc0..=0x1dff
                | 0x20d0..=0x20ff
                | 0xfe00..=0xfe0f
                | 0xfe20..=0xfe2f
                | 0xe0100..=0xe01ef
        )
    {
        return 0;
    }
    if matches!(
        code,
        0x1100..=0x115f
            | 0x2329..=0x232a
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe19
            | 0xfe30..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0x1f1e6..=0x1f1ff
            | 0x1f300..=0x1faff
            | 0x20000..=0x3fffd
    ) {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUGGESTIONS: &[TerminalSuggestion] = &[
        TerminalSuggestion {
            command: "/model [id]",
            description: "모델 변경",
        },
        TerminalSuggestion {
            command: "/help",
            description: "도움말",
        },
    ];

    #[test]
    fn filters_only_unfinished_slash_command_tokens() {
        assert_eq!(matching_suggestions("/", SUGGESTIONS).len(), 2);
        assert_eq!(
            matching_suggestions("/mo", SUGGESTIONS)[0].command,
            "/model [id]"
        );
        assert!(matching_suggestions("안녕", SUGGESTIONS).is_empty());
        assert!(matching_suggestions("/model ", SUGGESTIONS).is_empty());
    }

    #[test]
    fn backspace_removes_a_complete_utf8_character() {
        let mut input = "a한".as_bytes().to_vec();
        pop_last_utf8_char(&mut input);
        assert_eq!(String::from_utf8(input).unwrap(), "a");
    }
}
