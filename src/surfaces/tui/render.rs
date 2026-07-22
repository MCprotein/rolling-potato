use super::runtime_bridge::{TuiBackendStatus, TuiReadPage, TuiStatusSnapshot};
use super::view_model::{
    conversation_rows_per_page, notice_rows_per_page, ConversationRole, InteractiveState,
    InteractiveView,
};

const MAX_INTERACTIVE_WIDTH: usize = 120;
const BRAND_COLOR: &str = "\u{001b}[1;36m";
const ACCENT_COLOR: &str = "\u{001b}[36m";
const HEALTHY_COLOR: &str = "\u{001b}[32m";
const WARNING_COLOR: &str = "\u{001b}[33m";
const FAILED_COLOR: &str = "\u{001b}[31m";
const MUTED_COLOR: &str = "\u{001b}[2m";

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
    if matches!(state.view, InteractiveView::Conversation) {
        return render_conversation_frame(state, status, width, height, ansi_layout, color);
    }
    let content_rows = notice_rows_per_page(height);
    let notice_lines = state.notice.split('\n').collect::<Vec<_>>();
    let notice_page_count = notice_lines.len().div_ceil(content_rows).max(1);
    let notice_page = state.notice_page.min(notice_page_count - 1);
    let notice_offset = notice_page.saturating_mul(content_rows);
    let notice_rows = notice_lines
        .len()
        .saturating_sub(notice_offset)
        .min(content_rows);
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
    render_notice_lines(
        &mut output,
        &notice_lines,
        notice_offset,
        notice_rows,
        (notice_page, notice_page_count),
        width,
        NoticeStyle::Diagnostic,
    );
    output.push_str(&"-".repeat(width));
    output.push('\n');
    let status_line = render_status_line(status, width, color);
    render_composer(&mut output, &status_line, width, ansi_layout, color);
    output
}

fn render_conversation_frame(
    state: &InteractiveState,
    status: &TuiStatusSnapshot,
    width: usize,
    height: u16,
    ansi_layout: bool,
    color: bool,
) -> String {
    let mut output = String::new();
    if ansi_layout {
        output.push_str("\u{001b}[2J\u{001b}[H");
    }

    let show_welcome = state.turns.is_empty();
    if show_welcome {
        render_welcome(&mut output, status, width, color);
    } else {
        render_identity_header(&mut output, width, color);
        output.push('\n');
    }

    let content_rows = conversation_rows_per_page(height, show_welcome);
    let notice_lines = state.notice.split('\n').collect::<Vec<_>>();
    let notice_page_count = notice_lines.len().div_ceil(content_rows).max(1);
    let notice_page = state.notice_page.min(notice_page_count - 1);
    let notice_offset = notice_page.saturating_mul(content_rows);
    let notice_rows = if state.notice.is_empty() {
        0
    } else {
        notice_lines
            .len()
            .saturating_sub(notice_offset)
            .min(content_rows)
    };
    let turn_rows = content_rows.saturating_sub(notice_rows);
    let conversation = conversation_lines(state, width, color);
    let (visible_start, visible_end) = if turn_rows == 0 {
        (conversation.len(), conversation.len())
    } else {
        let page_count = conversation.len().div_ceil(turn_rows).max(1);
        let page_from_end = if state.notice.is_empty() {
            state.notice_page.min(page_count - 1)
        } else {
            0
        };
        let end = conversation
            .len()
            .saturating_sub(page_from_end.saturating_mul(turn_rows));
        (end.saturating_sub(turn_rows), end)
    };
    for line in &conversation[visible_start..visible_end] {
        output.push_str(line);
        output.push('\n');
    }
    render_notice_lines(
        &mut output,
        &notice_lines,
        notice_offset,
        notice_rows,
        (notice_page, notice_page_count),
        width,
        NoticeStyle::Conversation { color },
    );
    if ansi_layout {
        let rendered_rows = visible_end.saturating_sub(visible_start) + notice_rows;
        for _ in rendered_rows..content_rows {
            output.push('\n');
        }
    }

    let status_line = render_status_line(status, width, color);
    render_composer(&mut output, &status_line, width, ansi_layout, color);
    output
}

pub(crate) fn conversation_page_count(state: &InteractiveState, width: u16, height: u16) -> usize {
    let width = usize::from(width).clamp(20, MAX_INTERACTIVE_WIDTH);
    let rows = conversation_rows_per_page(height, state.turns.is_empty());
    conversation_lines(state, width, false)
        .len()
        .div_ceil(rows)
        .max(1)
}

fn conversation_lines(state: &InteractiveState, width: usize, color_enabled: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for turn in &state.turns {
        let (marker, color) = match turn.role {
            ConversationRole::User => ("›", BRAND_COLOR),
            ConversationRole::Assistant => ("●", "\u{001b}[1;32m"),
        };
        let mut first_row = true;
        for source_line in turn.content.split('\n') {
            let body_width = width.saturating_sub(2).max(1);
            for body in wrap_terminal_text(&sanitize_terminal_text(source_line), body_width) {
                let prefix = if first_row {
                    format!("{marker} ")
                } else {
                    "│ ".to_string()
                };
                lines.push(format!("{}{}", paint(&prefix, color, color_enabled), body));
                first_row = false;
            }
        }
        lines.push(String::new());
    }
    lines
}

fn render_composer(
    output: &mut String,
    status_line: &str,
    width: usize,
    ansi_layout: bool,
    color: bool,
) {
    if ansi_layout {
        output.push_str(&paint(
            &box_rule('╭', '╮', "─ 요청 ", width),
            MUTED_COLOR,
            color,
        ));
        output.push('\n');
        let inner_width = width.saturating_sub(2);
        output.push_str(&paint("│ ", MUTED_COLOR, color));
        output.push_str(&paint("› ", BRAND_COLOR, color));
        output.push_str(&" ".repeat(inner_width.saturating_sub(3)));
        output.push_str(&paint("│", MUTED_COLOR, color));
        output.push('\n');
        output.push_str(&paint(&box_rule('╰', '╯', "", width), MUTED_COLOR, color));
        output.push('\n');
        output.push_str(status_line);
        output.push('\n');
        output.push_str("\u{001b}[3A\r\u{001b}[4C");
    } else {
        output.push_str(status_line);
        output.push('\n');
        output.push_str(&paint("› ", BRAND_COLOR, color));
    }
}

fn render_welcome(output: &mut String, status: &TuiStatusSnapshot, width: usize, color: bool) {
    let title = format!(
        "─ rpotato v{} · 로컬 코딩 에이전트 ",
        env!("CARGO_PKG_VERSION")
    );
    output.push_str(&paint(
        &box_rule('╭', '╮', &title, width),
        BRAND_COLOR,
        color,
    ));
    output.push('\n');
    output.push_str(&box_row(
        &format!(" model    {}", sanitize_terminal_text(&status.model)),
        width,
    ));
    output.push('\n');
    output.push_str(&paint(
        &box_row(
            &format!(
                " project  {}",
                sanitize_terminal_text(&current_project_label())
            ),
            width,
        ),
        MUTED_COLOR,
        color,
    ));
    output.push('\n');
    output.push_str(&paint(
        &box_rule('╰', '╯', "─ /help 명령 · /model 변경 ", width),
        MUTED_COLOR,
        color,
    ));
    output.push('\n');
}

fn render_identity_header(output: &mut String, width: usize, color: bool) {
    let brand = format!("rpotato v{}", env!("CARGO_PKG_VERSION"));
    let separator = "  ·  ";
    let brand = truncate_chars(&brand, width);
    output.push_str(&paint(&brand, BRAND_COLOR, color));
    let used = display_cell_width(&brand);
    if used + display_cell_width(separator) < width {
        let remaining = width - used - display_cell_width(separator);
        let project = truncate_chars(&sanitize_terminal_text(&current_project_label()), remaining);
        output.push_str(&paint(separator, MUTED_COLOR, color));
        output.push_str(&paint(&project, MUTED_COLOR, color));
    }
    output.push('\n');
}

fn box_rule(left: char, right: char, label: &str, width: usize) -> String {
    if width <= 2 {
        return left.to_string().repeat(width);
    }
    let inner_width = width - 2;
    let label = truncate_chars(label, inner_width);
    let fill = inner_width.saturating_sub(display_cell_width(&label));
    format!("{left}{label}{}{right}", "─".repeat(fill))
}

fn box_row(content: &str, width: usize) -> String {
    if width <= 2 {
        return "│".repeat(width);
    }
    let inner_width = width - 2;
    let content = truncate_chars(content, inner_width);
    let padding = inner_width.saturating_sub(display_cell_width(&content));
    format!("│{content}{}│", " ".repeat(padding))
}

fn current_project_label() -> String {
    let path = std::env::var_os("RPOTATO_PROJECT_ROOT")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let display = path.display().to_string();
    let home = std::env::var("HOME").ok();
    home.and_then(|home| {
        display
            .strip_prefix(&home)
            .map(|suffix| format!("~{suffix}"))
    })
    .unwrap_or(display)
}

fn render_status_line(status: &TuiStatusSnapshot, width: usize, color: bool) -> String {
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
    let (compaction, compaction_color) = if status.has_compaction_checkpoint {
        ("compact saved", ACCENT_COLOR)
    } else if percent.is_some_and(|value| value >= 75) {
        ("compact due", WARNING_COLOR)
    } else {
        ("compact auto@75%", MUTED_COLOR)
    };
    let context_color = match percent {
        Some(value) if value >= 85 => FAILED_COLOR,
        Some(value) if value >= 60 => WARNING_COLOR,
        Some(_) => HEALTHY_COLOR,
        None => MUTED_COLOR,
    };
    let backend_color = match status.backend {
        TuiBackendStatus::Ready => HEALTHY_COLOR,
        TuiBackendStatus::Stopped => WARNING_COLOR,
        TuiBackendStatus::Stale => FAILED_COLOR,
        TuiBackendStatus::Unavailable => MUTED_COLOR,
    };
    let model_width = if width >= 96 {
        32
    } else if width >= 60 {
        20
    } else {
        12
    };
    let model = truncate_chars(&sanitize_terminal_text(&status.model), model_width);
    let session = short_status_id(&sanitize_terminal_text(&status.session_id));
    let segments = [
        (format!("model {model}"), ACCENT_COLOR),
        (context, context_color),
        (compaction.to_string(), compaction_color),
        (
            format!("backend {}", status.backend.as_str()),
            backend_color,
        ),
        (format!("session {session}"), MUTED_COLOR),
    ];
    render_status_segments(&segments, width, color)
}

fn render_status_segments(segments: &[(String, &str)], width: usize, color: bool) -> String {
    let separator = " | ";
    let mut output = String::new();
    let mut used = 0;
    for (index, (segment, code)) in segments.iter().enumerate() {
        let separator_width = usize::from(index > 0) * display_cell_width(separator);
        if used + separator_width >= width {
            break;
        }
        if index > 0 {
            output.push_str(&paint(separator, MUTED_COLOR, color));
            used += separator_width;
        }
        let remaining = width.saturating_sub(used);
        let visible = truncate_chars(segment, remaining);
        let visible_width = display_cell_width(&visible);
        output.push_str(&paint(&visible, code, color));
        used += visible_width;
        if visible_width < display_cell_width(segment) {
            break;
        }
    }
    output
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

fn wrap_terminal_text(value: &str, width: usize) -> Vec<String> {
    if value.is_empty() {
        return vec![String::new()];
    }
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let ch_width = terminal_cell_width(ch);
        if !current.is_empty() && used + ch_width > width {
            lines.push(current);
            current = String::new();
            used = 0;
        }
        current.push(ch);
        used += ch_width;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn render_notice_lines(
    output: &mut String,
    lines: &[&str],
    offset: usize,
    max_rows: usize,
    pagination: (usize, usize),
    width: usize,
    style: NoticeStyle,
) {
    let (page, page_count) = pagination;
    for (index, line) in lines.iter().skip(offset).take(max_rows).enumerate() {
        let prefix = match style {
            NoticeStyle::Diagnostic if index == 0 => "notice: ",
            NoticeStyle::Diagnostic => "        ",
            NoticeStyle::Conversation { .. } if index == 0 => "◇ ",
            NoticeStyle::Conversation { .. } => "  ",
        };
        let line = if index + 1 == max_rows && page_count > 1 {
            let separator = match style {
                NoticeStyle::Diagnostic => ";",
                NoticeStyle::Conversation { .. } => " ·",
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
            NoticeStyle::Diagnostic => output.push_str(prefix),
            NoticeStyle::Conversation { color } => {
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

#[derive(Clone, Copy)]
enum NoticeStyle {
    Diagnostic,
    Conversation { color: bool },
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
    if display_cell_width(value) <= width {
        return value.to_string();
    }
    if width == 0 {
        return String::new();
    }

    let available = width.saturating_sub(1);
    let mut used = 0;
    let mut out = String::new();
    for ch in value.chars() {
        let ch_width = terminal_cell_width(ch);
        if used + ch_width > available {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

pub(crate) fn display_cell_width(value: &str) -> usize {
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
