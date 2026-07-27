use super::runtime_bridge::{TuiAttachment, TuiReadPage, TuiStatusSnapshot};
use super::view_model::{
    conversation_rows_per_page, notice_rows_per_page, ConversationRole, InteractiveState,
    InteractiveView,
};

mod markdown;
mod report_layout;
mod status;
mod text;

pub(crate) use report_layout::{
    bytes_label, latency_label, percent_label, push_footer, push_header, push_kv,
    push_literal_block, push_rule, push_section, push_wrapped, short_id, terminal_width, tps_label,
};
pub(crate) use text::{display_cell_width, sanitize_terminal_text};
use text::{truncate_chars, wrap_terminal_text};

const MAX_READING_WIDTH: usize = 120;
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
    let width = usize::from(width).max(20);
    if matches!(state.view, InteractiveView::Conversation) {
        return render_conversation_frame(
            state,
            status,
            width,
            width.min(MAX_READING_WIDTH),
            height,
            ansi_layout,
            color,
        );
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
    let status_line = status::render_status_line(status, None, width, color);
    render_composer(
        &mut output,
        &state.attachments,
        &status_line,
        width,
        ansi_layout,
        color,
    );
    output
}

fn render_conversation_frame(
    state: &InteractiveState,
    status: &TuiStatusSnapshot,
    frame_width: usize,
    reading_width: usize,
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
        render_welcome(&mut output, status, frame_width, height > 10, color);
        if height > 10 {
            output.push('\n');
        }
    } else {
        render_identity_header(&mut output, frame_width, color);
        output.push('\n');
    }

    let content_rows = conversation_rows_per_page(height, show_welcome)
        .saturating_sub(usize::from(!state.attachments.is_empty()))
        .max(1);
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
    let conversation = conversation_lines(state, reading_width, color);
    let show_scroll_position = state.notice.is_empty() && state.notice_page > 0;
    let latest_rows = content_rows.saturating_sub(notice_rows).max(1);
    let scrolled_rows = latest_rows.saturating_sub(1).max(1);
    let (visible_start, visible_end) = if state.notice.is_empty() {
        conversation_window(
            conversation.len(),
            latest_rows,
            scrolled_rows,
            state.notice_page,
        )
    } else {
        let end = conversation.len();
        (end.saturating_sub(latest_rows), end)
    };
    if show_scroll_position {
        let page_count =
            conversation_page_count_for_rows(conversation.len(), latest_rows, scrolled_rows);
        let page_from_end = state.notice_page.min(page_count - 1);
        output.push_str(&paint(
            &format!(
                "↑ 이전 대화 · {}/{} · PageDown/휠↓ 최신",
                page_from_end + 1,
                page_count
            ),
            MUTED_COLOR,
            color,
        ));
        output.push('\n');
    }
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
        reading_width,
        NoticeStyle::Conversation { color },
    );
    if ansi_layout {
        let rendered_rows = visible_end.saturating_sub(visible_start)
            + notice_rows
            + usize::from(show_scroll_position);
        for _ in rendered_rows..content_rows {
            output.push('\n');
        }
    }

    let status_line =
        status::render_status_line(status, state.context_tokens_estimate, frame_width, color);
    render_composer(
        &mut output,
        &state.attachments,
        &status_line,
        frame_width,
        ansi_layout,
        color,
    );
    output
}

pub(crate) fn conversation_page_count(state: &InteractiveState, width: u16, height: u16) -> usize {
    let width = usize::from(width).clamp(20, MAX_READING_WIDTH);
    let latest_rows = conversation_rows_per_page(height, state.turns.is_empty())
        .saturating_sub(usize::from(!state.attachments.is_empty()))
        .max(1);
    conversation_page_count_for_rows(
        conversation_lines(state, width, false).len(),
        latest_rows,
        latest_rows.saturating_sub(1).max(1),
    )
}

fn conversation_page_count_for_rows(
    line_count: usize,
    latest_rows: usize,
    scrolled_rows: usize,
) -> usize {
    if line_count <= latest_rows {
        1
    } else {
        1 + line_count
            .saturating_sub(latest_rows)
            .div_ceil(scrolled_rows.max(1))
    }
}

fn conversation_window(
    line_count: usize,
    latest_rows: usize,
    scrolled_rows: usize,
    page_from_end: usize,
) -> (usize, usize) {
    let page_count =
        conversation_page_count_for_rows(line_count, latest_rows, scrolled_rows).max(1);
    let page_from_end = page_from_end.min(page_count - 1);
    if page_from_end == 0 {
        return (line_count.saturating_sub(latest_rows), line_count);
    }
    let end = line_count
        .saturating_sub(latest_rows)
        .saturating_sub((page_from_end - 1).saturating_mul(scrolled_rows));
    (end.saturating_sub(scrolled_rows), end)
}

fn conversation_lines(state: &InteractiveState, width: usize, color_enabled: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for turn in &state.turns {
        let (marker, color) = match turn.role {
            ConversationRole::User => ("›", BRAND_COLOR),
            ConversationRole::Assistant => ("●", "\u{001b}[1;32m"),
            ConversationRole::Error => ("×", FAILED_COLOR),
        };
        let mut first_row = true;
        let mut markdown = markdown::MarkdownState::default();
        for source_line in turn.content.split('\n') {
            let body_width = width.saturating_sub(2).max(1);
            let bodies = if turn.role == ConversationRole::Assistant {
                markdown.render_line(source_line, body_width, color_enabled)
            } else {
                wrap_terminal_text(&sanitize_terminal_text(source_line), body_width)
            };
            for body in bodies {
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
    attachments: &[TuiAttachment],
    status_line: &str,
    width: usize,
    ansi_layout: bool,
    color: bool,
) {
    if !attachments.is_empty() {
        let labels = attachments
            .iter()
            .map(|attachment| {
                format!(
                    "[{}: {}]",
                    attachment.kind.label(),
                    sanitize_terminal_text(&attachment.display_name)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        output.push_str(&paint(
            &truncate_chars(&format!("첨부 {labels}"), width),
            ACCENT_COLOR,
            color,
        ));
        output.push('\n');
    }
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

fn render_welcome(
    output: &mut String,
    status: &TuiStatusSnapshot,
    width: usize,
    show_session_hint: bool,
    color: bool,
) {
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
    if show_session_hint {
        output.push_str(&paint(
            &box_row(" session  새 대화 · /resume으로 이전 대화 재개", width),
            ACCENT_COLOR,
            color,
        ));
        output.push('\n');
    }
    output.push_str(&paint(
        &box_rule(
            '╰',
            '╯',
            "─ /help 명령 · /model 변경 · /new 새 대화 ",
            width,
        ),
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

fn paint(value: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{value}\u{001b}[0m")
    } else {
        value.to_string()
    }
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
