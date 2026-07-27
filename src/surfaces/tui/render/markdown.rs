//! Bounded terminal presentation for the small Markdown subset used in chat answers.

use super::text::{
    sanitize_terminal_text, terminal_cell_width, truncate_chars, wrap_terminal_text,
};

const BOLD_ON: &str = "\u{001b}[1m";
const BOLD_OFF: &str = "\u{001b}[22m";
const CODE_ON: &str = "\u{001b}[36m";
const STYLE_OFF: &str = "\u{001b}[0m";

#[derive(Default)]
pub(super) struct MarkdownState {
    code_fence: bool,
}

impl MarkdownState {
    pub(super) fn render_line(&mut self, source: &str, width: usize, color: bool) -> Vec<String> {
        let source = sanitize_terminal_text(source);
        if let Some(language) = source.trim().strip_prefix("```") {
            if self.code_fence {
                self.code_fence = false;
                return vec![truncate_chars("└─", width)];
            }
            self.code_fence = true;
            let language = language.trim();
            let header = if language.is_empty() {
                "┌─ code".to_string()
            } else {
                format!("┌─ code · {language}")
            };
            return vec![truncate_chars(&header, width)];
        }
        if self.code_fence {
            return wrap_terminal_text(&format!("│ {source}"), width);
        }

        let semantic = semantic_line(&source);
        render_inline_wrapped(&semantic, width, color)
    }
}

fn semantic_line(source: &str) -> String {
    let trimmed = source.trim_start();
    let heading = trimmed
        .strip_prefix("### ")
        .or_else(|| trimmed.strip_prefix("## "))
        .or_else(|| trimmed.strip_prefix("# "));
    if let Some(heading) = heading {
        return format!("**{heading}**");
    }
    if let Some(item) = trimmed
        .strip_prefix("* ")
        .or_else(|| trimmed.strip_prefix("- "))
    {
        return format!("• {item}");
    }
    source.to_string()
}

fn render_inline_wrapped(source: &str, width: usize, color: bool) -> Vec<String> {
    if source.is_empty() {
        return vec![String::new()];
    }
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut output = String::new();
    let mut used = 0;
    let mut index = 0;
    let mut bold = false;
    let mut code = false;
    while index < source.len() {
        let tail = &source[index..];
        if tail.starts_with("**") {
            bold = !bold;
            if color {
                output.push_str(if bold { BOLD_ON } else { BOLD_OFF });
            }
            index += 2;
            continue;
        }
        if tail.starts_with('`') {
            code = !code;
            if color {
                output.push_str(if code { CODE_ON } else { STYLE_OFF });
                if !code && bold {
                    output.push_str(BOLD_ON);
                }
            }
            index += 1;
            continue;
        }
        let character = tail.chars().next().expect("non-empty markdown tail");
        let character_width = terminal_cell_width(character);
        if used > 0 && used + character_width > width {
            close_active_styles(&mut output, bold || code, color);
            lines.push(output);
            output = active_style_prefix(bold, code, color);
            used = 0;
        }
        output.push(character);
        used += character_width;
        index += character.len_utf8();
    }
    close_active_styles(&mut output, bold || code, color);
    if !output.is_empty() {
        lines.push(output);
    }
    lines
}

fn active_style_prefix(bold: bool, code: bool, color: bool) -> String {
    if !color {
        return String::new();
    }
    let mut output = String::new();
    if bold {
        output.push_str(BOLD_ON);
    }
    if code {
        output.push_str(CODE_ON);
    }
    output
}

fn close_active_styles(output: &mut String, active: bool, color: bool) {
    if color && active {
        output.push_str(STYLE_OFF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_subset_removes_markers_and_keeps_terminal_styles_bounded() {
        let mut state = MarkdownState::default();

        assert_eq!(
            state.render_line("* **Qwen**: `coding`", 80, false),
            ["• Qwen: coding"]
        );
        let colored = state.render_line("## 비교", 80, true).join("");
        assert!(colored.contains("\u{001b}[1m비교\u{001b}[22m"));
    }

    #[test]
    fn inline_styles_survive_wrapping_without_counting_markers_as_cells() {
        let mut state = MarkdownState::default();

        let lines = state.render_line("**가나다라마바사아자차**", 6, true);

        assert!(lines.len() >= 3);
        assert!(lines
            .iter()
            .all(|line| display_width_without_ansi(line) <= 6));
        assert!(lines.iter().all(|line| line.contains(BOLD_ON)));
        assert!(!lines.join("").contains("**"));
    }

    #[test]
    fn code_fence_language_header_is_bounded_to_the_viewport() {
        let mut state = MarkdownState::default();

        let header = state.render_line(
            "```an-extremely-long-language-identifier-that-must-not-overflow",
            18,
            false,
        );

        assert_eq!(header.len(), 1);
        assert!(super::super::text::display_cell_width(&header[0]) <= 18);
        assert!(header[0].ends_with('…'));
    }

    fn display_width_without_ansi(value: &str) -> usize {
        let mut visible = String::new();
        let mut chars = value.chars().peekable();
        while let Some(character) = chars.next() {
            if character == '\u{001b}' && chars.next_if_eq(&'[').is_some() {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            } else {
                visible.push(character);
            }
        }
        super::super::text::display_cell_width(&visible)
    }
}
