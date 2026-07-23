use std::io::{self, Write};

use crate::runtime_core::terminal::{TerminalFault, TerminalSuggestion};

use super::{editor::Editor, visible_suggestions};

pub(super) fn redraw(
    editor: &Editor,
    suggestions: &[TerminalSuggestion],
    terminal_width: usize,
    base_frame: &str,
) -> Result<(), TerminalFault> {
    let matches = visible_suggestions(editor, suggestions);
    let input_width = terminal_width.saturating_sub(6).max(1);
    let (visible, cursor_width) = visible_window(editor.text(), editor.cursor(), input_width);
    let command_width = matches
        .iter()
        .map(|item| display_cell_width(item.command))
        .max()
        .unwrap_or(0);
    let mut output = String::from(base_frame);
    output.push_str("\r\u{001b}[4C");
    output.push_str(visible);
    output.push_str(&" ".repeat(input_width.saturating_sub(display_cell_width(visible))));
    output.push_str("\u{001b}[2m│\u{001b}[0m\r");
    output.push_str(&format!("\u{001b}[{}C\u{001b}7", 4 + cursor_width));
    for (row, entry) in matches.iter().enumerate() {
        output.push_str("\u{001b}8");
        output.push_str(&format!(
            "\u{001b}[{}A\r\u{001b}[2K",
            2 + matches.len() - row - 1
        ));
        let marker = if row == editor.selected.min(matches.len().saturating_sub(1)) {
            "› "
        } else {
            "  "
        };
        let padding = command_width.saturating_sub(display_cell_width(entry.command));
        let description = truncate_plain_text(
            entry.description,
            terminal_width.saturating_sub(4 + command_width + 2),
        );
        output.push_str(&format!(
            "{marker}\u{001b}[1;36m{}\u{001b}[0m{}  \u{001b}[2m{}\u{001b}[0m",
            entry.command,
            " ".repeat(padding),
            description
        ));
    }
    output.push_str("\u{001b}8");
    write_stdout(&output)
}

fn visible_window(value: &str, cursor: usize, width: usize) -> (&str, usize) {
    let mut start = cursor;
    let mut used = 0;
    for (index, ch) in value[..cursor].char_indices().rev() {
        let next = used + terminal_cell_width(ch);
        if next > width {
            break;
        }
        used = next;
        start = index;
    }
    let mut end = start;
    let mut visible = 0;
    for (offset, ch) in value[start..].char_indices() {
        let next = visible + terminal_cell_width(ch);
        if next > width {
            break;
        }
        visible = next;
        end = start + offset + ch.len_utf8();
    }
    (
        &value[start..end],
        display_cell_width(&value[start..cursor]),
    )
}

fn truncate_plain_text(value: &str, width: usize) -> String {
    if display_cell_width(value) <= width {
        return value.to_string();
    }
    let mut output = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let next = used + terminal_cell_width(ch);
        if next > width.saturating_sub(1) {
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
    if ch.is_control() || ch == '\u{200d}' || ('\u{0300}'..='\u{036f}').contains(&ch) {
        return 0;
    }
    if ch >= '\u{1100}'
        && matches!(
            ch as u32,
            0x1100..=0x115f
                | 0x2e80..=0xa4cf
                | 0xac00..=0xd7a3
                | 0xf900..=0xfaff
                | 0x1f300..=0x1faff
                | 0x20000..=0x3fffd
        )
    {
        2
    } else {
        1
    }
}

fn write_stdout(value: &str) -> Result<(), TerminalFault> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(value.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|_| TerminalFault::FrameWrite)
}

pub(super) struct BracketedPasteGuard;

impl BracketedPasteGuard {
    pub(super) fn start() -> Result<Self, TerminalFault> {
        write_stdout("\u{001b}[?2004h")?;
        Ok(Self)
    }
}

impl Drop for BracketedPasteGuard {
    fn drop(&mut self) {
        let _ = write_stdout("\u{001b}[?2004l");
    }
}
