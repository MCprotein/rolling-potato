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
    let indent = &source[..source.len().saturating_sub(trimmed.len())];
    let heading = trimmed
        .strip_prefix("### ")
        .or_else(|| trimmed.strip_prefix("## "))
        .or_else(|| trimmed.strip_prefix("# "));
    if let Some(heading) = heading {
        return normalize_links(&format!("**{heading}**"));
    }
    if let Some(item) = trimmed
        .strip_prefix("* ")
        .or_else(|| trimmed.strip_prefix("- "))
        .or_else(|| trimmed.strip_prefix("+ "))
    {
        return normalize_links(&format!("{indent}• {item}"));
    }
    if let Some((number, item)) = ordered_list_item(trimmed) {
        return normalize_links(&format!("{indent}{number}. {item}"));
    }
    if let Some(quote) = trimmed.strip_prefix("> ") {
        return normalize_links(&format!("{indent}│ {quote}"));
    }
    if is_table_separator(trimmed) {
        return render_table_separator(trimmed);
    }
    if let Some(row) = render_table_row(trimmed) {
        return normalize_links(&format!("{indent}{row}"));
    }
    normalize_links(source)
}

fn ordered_list_item(source: &str) -> Option<(&str, &str)> {
    let dot = source.find(". ")?;
    let number = &source[..dot];
    if number.is_empty() || !number.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    Some((number, &source[dot + 2..]))
}

fn is_table_separator(source: &str) -> bool {
    table_cells(source).is_some_and(|cells| {
        cells.iter().all(|cell| {
            let cell = cell.trim().trim_matches(':');
            cell.len() >= 3 && cell.chars().all(|character| character == '-')
        })
    })
}

fn render_table_separator(source: &str) -> String {
    table_cells(source)
        .unwrap_or_default()
        .iter()
        .map(|cell| {
            let separator = cell.trim().trim_matches(':');
            "─".repeat(separator.chars().count().clamp(3, 24))
        })
        .collect::<Vec<_>>()
        .join("─┼─")
}

fn render_table_row(source: &str) -> Option<String> {
    let cells = table_cells(source)?;
    Some(
        cells
            .iter()
            .map(|cell| cell.trim())
            .collect::<Vec<_>>()
            .join(" │ "),
    )
}

fn table_cells(source: &str) -> Option<Vec<&str>> {
    if !source.contains('|') {
        return None;
    }
    let cells = source
        .trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .collect::<Vec<_>>();
    (cells.len() >= 2).then_some(cells)
}

fn normalize_links(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut remaining = source;
    let mut inline_code = false;
    while let Some(character) = remaining.chars().next() {
        if character == '`' {
            inline_code = !inline_code;
            output.push(character);
            remaining = &remaining[character.len_utf8()..];
            continue;
        }
        if !inline_code && character == '[' {
            if let Some(label_end) = remaining.find("](") {
                if let Some(target_end) = remaining[label_end + 2..].find(')') {
                    let label = &remaining[1..label_end];
                    let target_start = label_end + 2;
                    let target_end = target_start + target_end;
                    let target = &remaining[target_start..target_end];
                    if !label.is_empty() && visible_link_target(target) {
                        output.push_str(label);
                        output.push_str(" (");
                        output.push_str(target);
                        output.push(')');
                        remaining = &remaining[target_end + 1..];
                        continue;
                    }
                }
            }
        }
        output.push(character);
        remaining = &remaining[character.len_utf8()..];
    }
    output
}

fn visible_link_target(target: &str) -> bool {
    let target = target.trim();
    !target.is_empty()
        && !target.chars().any(char::is_control)
        && (target.starts_with("https://")
            || target.starts_with("http://")
            || target.starts_with('/')
            || target.starts_with("./")
            || target.starts_with("../")
            || target.starts_with('#'))
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

    #[test]
    fn links_tables_quotes_and_nested_lists_render_without_raw_markers() {
        let mut state = MarkdownState::default();

        assert_eq!(
            state.render_line("- [공식 문서](https://example.com/docs)", 120, false),
            ["• 공식 문서 (https://example.com/docs)"]
        );
        assert_eq!(
            state.render_line("  - 중첩 항목", 120, false),
            ["  • 중첩 항목"]
        );
        assert_eq!(
            state.render_line("> 주의 사항", 120, false),
            ["│ 주의 사항"]
        );
        assert_eq!(
            state.render_line("| 모델 | 상태 |", 120, false),
            ["모델 │ 상태"]
        );
        assert_eq!(
            state.render_line("| --- | :---: |", 120, false),
            ["────┼────"]
        );
        assert_eq!(
            state.render_line("2. 다음 단계", 120, false),
            ["2. 다음 단계"]
        );
    }

    #[test]
    fn inline_code_does_not_interpret_markdown_links() {
        let mut state = MarkdownState::default();

        assert_eq!(
            state.render_line("`[label](https://example.com)`", 120, false),
            ["[label](https://example.com)"]
        );
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
