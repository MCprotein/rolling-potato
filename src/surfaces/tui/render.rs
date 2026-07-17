use super::runtime_bridge::TuiReadPage;
use super::view_model::InteractiveState;

const MAX_INTERACTIVE_WIDTH: usize = 120;

pub(crate) fn render_interactive_frame(
    state: &InteractiveState,
    page: &TuiReadPage,
    width: u16,
    height: u16,
) -> String {
    let width = usize::from(width).clamp(20, MAX_INTERACTIVE_WIDTH);
    let body_rows = usize::from(height).saturating_sub(5).max(1);
    let mut output = String::new();
    output.push_str(&format!(
        "rpotato interactive | {} | page {} | freshness {} | continuation {}\n",
        sanitize_terminal_text(&page.title),
        page.page + 1,
        page.freshness.as_str(),
        page.continuation.as_str(),
    ));
    output.push_str(&"-".repeat(width));
    output.push('\n');
    for line in page.lines.iter().take(body_rows) {
        output.push_str(&truncate_chars(&sanitize_terminal_text(line), width));
        output.push('\n');
    }
    render_notice_lines(&mut output, &state.notice, width);
    output.push_str("rpotato> ");
    output
}

fn render_notice_lines(output: &mut String, notice: &str, width: usize) {
    for (index, line) in notice.split('\n').enumerate() {
        let prefix = if index == 0 { "notice: " } else { "        " };
        output.push_str(prefix);
        output.push_str(&truncate_chars(
            &sanitize_terminal_text(line),
            width.saturating_sub(prefix.len()),
        ));
        output.push('\n');
    }
}

pub(crate) fn sanitize_terminal_text(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{001b}' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    let mut escaped = false;
                    for next in chars.by_ref() {
                        if next == '\u{0007}' || (escaped && next == '\\') {
                            break;
                        }
                        escaped = next == '\u{001b}';
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
            out.push_str("<esc>");
        } else if ch.is_control() {
            match ch {
                '\n' => out.push_str("<lf>"),
                '\r' => out.push_str("<cr>"),
                '\t' => out.push_str("  "),
                _ => out.push_str("<ctl>"),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn truncate_chars(value: &str, width: usize) -> String {
    let count = value.chars().count();
    if count <= width {
        return value.to_string();
    }
    if width <= 1 {
        return "…".chars().take(width).collect();
    }
    let mut out = value.chars().take(width - 1).collect::<String>();
    out.push('…');
    out
}
