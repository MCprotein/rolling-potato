use super::runtime_bridge::{TuiReadPage, TuiStatusSnapshot};
use super::view_model::{notice_rows_per_page, InteractiveState, InteractiveView};

mod chrome;
mod conversation;
mod markdown;
mod notice;
mod report_layout;
mod status;
mod text;

pub(crate) use report_layout::{
    bytes_label, latency_label, percent_label, push_footer, push_header, push_kv,
    push_literal_block, push_rule, push_section, push_wrapped, short_id, terminal_width, tps_label,
};
pub(crate) use text::{display_cell_width, sanitize_terminal_text};
use text::{truncate_chars, wrap_terminal_text};

pub(super) const BRAND_COLOR: &str = "\u{001b}[1;36m";
pub(super) const ACCENT_COLOR: &str = "\u{001b}[36m";
pub(super) const HEALTHY_COLOR: &str = "\u{001b}[32m";
pub(super) const WARNING_COLOR: &str = "\u{001b}[33m";
pub(super) const FAILED_COLOR: &str = "\u{001b}[31m";
pub(super) const MUTED_COLOR: &str = "\u{001b}[2m";

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
        return conversation::render_frame(state, status, width, height, ansi_layout, color);
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
    notice::render_lines(
        &mut output,
        &notice_lines,
        notice_offset,
        notice_rows,
        (notice_page, notice_page_count),
        width,
        notice::Style::Diagnostic,
    );
    output.push_str(&"-".repeat(width));
    output.push('\n');
    let status_line = status::render_status_line(status, None, width, color);
    chrome::render_composer(
        &mut output,
        &state.attachments,
        &status_line,
        width,
        ansi_layout,
        color,
    );
    output
}

pub(crate) fn conversation_page_count(state: &InteractiveState, width: u16, height: u16) -> usize {
    conversation::page_count(state, width, height)
}

pub(super) fn paint(value: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{value}\u{001b}[0m")
    } else {
        value.to_string()
    }
}
