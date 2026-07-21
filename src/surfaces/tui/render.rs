use super::runtime_bridge::{TuiBackendStatus, TuiReadPage, TuiStatusSnapshot};
use super::view_model::InteractiveState;

const MAX_INTERACTIVE_WIDTH: usize = 120;

#[cfg(test)]
pub(crate) fn render_interactive_frame(
    state: &InteractiveState,
    page: &TuiReadPage,
    width: u16,
    height: u16,
) -> String {
    render_interactive_frame_with_options(
        state,
        page,
        &TuiStatusSnapshot::unavailable(),
        width,
        height,
        false,
        false,
    )
}

pub(crate) fn render_interactive_frame_with_options(
    state: &InteractiveState,
    page: &TuiReadPage,
    status: &TuiStatusSnapshot,
    width: u16,
    height: u16,
    ansi_layout: bool,
    color: bool,
) -> String {
    let ansi_layout = ansi_layout && color;
    let width = usize::from(width).clamp(20, MAX_INTERACTIVE_WIDTH);
    let content_rows = usize::from(height).saturating_sub(7).max(1);
    let notice_line_count = state.notice.split('\n').count();
    let notice_rows = notice_line_count.min(content_rows);
    let body_rows = content_rows.saturating_sub(notice_rows);
    let mut output = String::new();
    if ansi_layout {
        output.push_str("\u{001b}[2J\u{001b}[H");
    }
    let header = format!(
        "rpotato | {} | page {} | freshness {} | continuation {}\n",
        sanitize_terminal_text(&page.title),
        page.page + 1,
        page.freshness.as_str(),
        page.continuation.as_str(),
    );
    output.push_str(&paint(&header, "\u{001b}[1;36m", color));
    output.push_str(&"-".repeat(width));
    output.push('\n');
    for line in page.lines.iter().take(body_rows) {
        output.push_str(&truncate_chars(&sanitize_terminal_text(line), width));
        output.push('\n');
    }
    render_notice_lines(&mut output, &state.notice, width, notice_rows);
    output.push_str(&"-".repeat(width));
    output.push('\n');
    let status_line = render_status_line(status, width);
    if ansi_layout {
        output.push_str("rpotato> \n");
        output.push_str(&paint_status_line(&status_line, status.backend, color));
        output.push('\n');
        output.push_str("\u{001b}[2A\r\u{001b}[9C");
    } else {
        output.push_str(&paint_status_line(&status_line, status.backend, color));
        output.push('\n');
        output.push_str("rpotato> ");
    }
    output
}

fn render_status_line(status: &TuiStatusSnapshot, width: usize) -> String {
    let (context, percent) = match (status.context_tokens_used, status.context_limit_tokens) {
        (Some(used), Some(limit)) if limit > 0 => {
            let percent = used.saturating_mul(100) / limit;
            (format!("ctx {used}/{limit} ({percent}%)"), Some(percent))
        }
        (Some(used), Some(_)) => (format!("ctx {used}/—"), None),
        (Some(used), None) => (format!("ctx {used}/—"), None),
        (None, Some(limit)) => (format!("ctx —/{limit}"), None),
        (None, None) => ("ctx —".to_string(), None),
    };
    let compaction = if status.has_compaction_checkpoint {
        "compact saved"
    } else if percent.is_some_and(|value| value >= 75) {
        "compact due"
    } else {
        "compact auto@75%"
    };
    truncate_chars(
        &format!(
            "model {} | {} | {} | backend {} | session {}",
            sanitize_terminal_text(&status.model),
            context,
            compaction,
            status.backend.as_str(),
            short_status_id(&sanitize_terminal_text(&status.session_id))
        ),
        width,
    )
}

fn paint_status_line(value: &str, backend: TuiBackendStatus, color: bool) -> String {
    let code = match backend {
        TuiBackendStatus::Ready => "\u{001b}[32m",
        TuiBackendStatus::Stopped | TuiBackendStatus::Unavailable => "\u{001b}[2m",
        TuiBackendStatus::Stale => "\u{001b}[31m",
    };
    paint(value, code, color)
}

fn paint(value: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{value}\u{001b}[0m")
    } else {
        value.to_string()
    }
}

fn short_status_id(value: &str) -> String {
    if value.chars().count() <= 12 {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(11).collect::<String>())
    }
}

fn render_notice_lines(output: &mut String, notice: &str, width: usize, max_rows: usize) {
    let lines = notice.split('\n').collect::<Vec<_>>();
    for (index, line) in lines.iter().take(max_rows).enumerate() {
        let prefix = if index == 0 { "notice: " } else { "        " };
        let line = if index + 1 == max_rows && lines.len() > max_rows {
            format!("{line} …")
        } else {
            (*line).to_string()
        };
        output.push_str(prefix);
        output.push_str(&truncate_chars(
            &sanitize_terminal_text(&line),
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

const DEFAULT_REPORT_WIDTH: usize = 92;
const MIN_REPORT_WIDTH: usize = 64;
const MAX_REPORT_WIDTH: usize = 120;

pub(crate) fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_REPORT_WIDTH)
        .clamp(MIN_REPORT_WIDTH, MAX_REPORT_WIDTH)
}

pub(crate) fn push_header(lines: &mut Vec<String>, width: usize, title: &str) {
    push_border(lines, width, '=');
    push_center(lines, width, title);
    push_border(lines, width, '=');
}

pub(crate) fn push_footer(lines: &mut Vec<String>, width: usize) {
    push_border(lines, width, '=');
    push_wrapped(
    lines,
    width,
    "beta boundary: this TUI surface reads runtime state only and does not approve, apply, resume, cancel, or mutate workflows.",
);
}

pub(crate) fn push_section(lines: &mut Vec<String>, width: usize, label: &str) {
    push_wrapped(lines, width, &format!("[{label}]"));
}

pub(crate) fn push_rule(lines: &mut Vec<String>, width: usize) {
    push_border(lines, width, '-');
}

pub(crate) fn push_border(lines: &mut Vec<String>, width: usize, ch: char) {
    lines.push(ch.to_string().repeat(width));
}

pub(crate) fn push_center(lines: &mut Vec<String>, width: usize, value: &str) {
    let value = truncate(value, width);
    let padding = width.saturating_sub(value.len()) / 2;
    lines.push(format!("{}{}", " ".repeat(padding), value));
}

pub(crate) fn push_kv(lines: &mut Vec<String>, width: usize, key: &str, value: &str) {
    push_wrapped(lines, width, &format!("{key}: {value}"));
}

pub(crate) fn push_wrapped(lines: &mut Vec<String>, width: usize, value: &str) {
    let mut current = String::new();
    for word in value.split_whitespace() {
        let next_len = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if next_len > width && !current.is_empty() {
            lines.push(truncate(&current, width));
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if current.is_empty() {
        lines.push(String::new());
    } else {
        lines.push(truncate(&current, width));
    }
}

pub(crate) fn push_literal_block(lines: &mut Vec<String>, width: usize, value: &str) {
    for line in value.lines() {
        lines.push(truncate(line, width));
    }
    if value.is_empty() {
        lines.push(String::new());
    }
}

pub(crate) fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let prefix = value.chars().take(width - 3).collect::<String>();
    format!("{prefix}...")
}

pub(crate) fn latency_label(value: Option<f64>) -> String {
    value
        .map(|latency| format!("{latency:.1}ms"))
        .unwrap_or_else(|| "not recorded".to_string())
}

pub(crate) fn tps_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1} tok/s"))
        .unwrap_or_else(|| "not recorded".to_string())
}

pub(crate) fn percent_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn bytes_label(value: Option<u64>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = value as f64;
    if value >= GIB {
        format!("{:.1} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{value:.0} B")
    }
}

pub(crate) fn short_id(value: &str) -> String {
    if value.len() <= 18 {
        return value.to_string();
    }
    format!("{}...", &value[..18])
}
