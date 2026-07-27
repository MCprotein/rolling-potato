//! Bounded terminal presentation for the small Markdown subset used in chat answers.

use super::text::{sanitize_terminal_text, wrap_terminal_text};

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
                return vec!["└─".to_string()];
            }
            self.code_fence = true;
            let language = language.trim();
            return vec![if language.is_empty() {
                "┌─ code".to_string()
            } else {
                format!("┌─ code · {language}")
            }];
        }
        if self.code_fence {
            return wrap_terminal_text(&format!("│ {source}"), width);
        }

        let semantic = semantic_line(&source);
        wrap_terminal_text(&semantic, width)
            .into_iter()
            .map(|line| apply_inline_styles(&line, color))
            .collect()
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

fn apply_inline_styles(source: &str, color: bool) -> String {
    let mut output = String::new();
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
        output.push(character);
        index += character.len_utf8();
    }
    if color && (bold || code) {
        output.push_str(STYLE_OFF);
    }
    output
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
}
