//! Paginated notice rendering for diagnostic and conversation surfaces.

use super::{display_cell_width, paint, sanitize_terminal_text, truncate_chars, ACCENT_COLOR};

#[derive(Clone, Copy)]
pub(super) enum Style {
    Diagnostic,
    Conversation { color: bool },
}

pub(super) fn render_lines(
    output: &mut String,
    lines: &[&str],
    offset: usize,
    max_rows: usize,
    pagination: (usize, usize),
    width: usize,
    style: Style,
) {
    let (page, page_count) = pagination;
    for (index, line) in lines.iter().skip(offset).take(max_rows).enumerate() {
        let prefix = match style {
            Style::Diagnostic if index == 0 => "notice: ",
            Style::Diagnostic => "        ",
            Style::Conversation { .. } if index == 0 => "◇ ",
            Style::Conversation { .. } => "  ",
        };
        let line = if index + 1 == max_rows && page_count > 1 {
            let separator = match style {
                Style::Diagnostic => ";",
                Style::Conversation { .. } => " ·",
            };
            format!(
                "{line} … [{}/{}{separator} /more /back]",
                page + 1,
                page_count
            )
        } else {
            (*line).to_string()
        };
        match style {
            Style::Diagnostic => output.push_str(prefix),
            Style::Conversation { color } => {
                output.push_str(&paint(prefix, ACCENT_COLOR, color));
            }
        }
        output.push_str(&truncate_chars(
            &sanitize_terminal_text(&line),
            width.saturating_sub(display_cell_width(prefix)),
        ));
        output.push('\n');
    }
}
