//! Conversation transcript rendering and viewport pagination.

use super::{
    chrome, markdown, notice, paint, status, wrap_terminal_text, BRAND_COLOR, FAILED_COLOR,
    MUTED_COLOR,
};
use crate::surfaces::tui::runtime_bridge::TuiStatusSnapshot;
use crate::surfaces::tui::view_model::{
    conversation_rows_per_page, ConversationRole, InteractiveState,
};

const MAX_READING_WIDTH: usize = 120;

pub(super) fn render_frame(
    state: &InteractiveState,
    status_snapshot: &TuiStatusSnapshot,
    frame_width: usize,
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
        chrome::render_welcome(
            &mut output,
            status_snapshot,
            frame_width,
            height > 10,
            color,
        );
        if height > 10 {
            output.push('\n');
        }
    } else {
        chrome::render_identity_header(&mut output, frame_width, color);
        output.push('\n');
    }

    let reading_width = frame_width.min(MAX_READING_WIDTH);
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
    let conversation = lines(state, reading_width, color);
    let show_scroll_position = state.notice.is_empty() && state.notice_page > 0;
    let latest_rows = content_rows.saturating_sub(notice_rows).max(1);
    let scrolled_rows = latest_rows.saturating_sub(1).max(1);
    let (visible_start, visible_end) = if state.notice.is_empty() {
        window(
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
        let total_pages = page_count_for_rows(conversation.len(), latest_rows, scrolled_rows);
        let page_from_end = state.notice_page.min(total_pages - 1);
        output.push_str(&paint(
            &format!(
                "↑ 이전 대화 · {}/{} · PageDown/휠↓ 최신",
                page_from_end + 1,
                total_pages
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
    notice::render_lines(
        &mut output,
        &notice_lines,
        notice_offset,
        notice_rows,
        (notice_page, notice_page_count),
        reading_width,
        notice::Style::Conversation { color },
    );
    if ansi_layout {
        let rendered_rows = visible_end.saturating_sub(visible_start)
            + notice_rows
            + usize::from(show_scroll_position);
        for _ in rendered_rows..content_rows {
            output.push('\n');
        }
    }

    let status_line = status::render_status_line(
        status_snapshot,
        state.context_tokens_estimate,
        frame_width,
        color,
    );
    chrome::render_composer(
        &mut output,
        &state.attachments,
        &status_line,
        frame_width,
        ansi_layout,
        color,
    );
    output
}

pub(super) fn page_count(state: &InteractiveState, width: u16, height: u16) -> usize {
    let width = usize::from(width).clamp(20, MAX_READING_WIDTH);
    let latest_rows = conversation_rows_per_page(height, state.turns.is_empty())
        .saturating_sub(usize::from(!state.attachments.is_empty()))
        .max(1);
    page_count_for_rows(
        lines(state, width, false).len(),
        latest_rows,
        latest_rows.saturating_sub(1).max(1),
    )
}

fn page_count_for_rows(line_count: usize, latest_rows: usize, scrolled_rows: usize) -> usize {
    if line_count <= latest_rows {
        1
    } else {
        1 + line_count
            .saturating_sub(latest_rows)
            .div_ceil(scrolled_rows.max(1))
    }
}

fn window(
    line_count: usize,
    latest_rows: usize,
    scrolled_rows: usize,
    page_from_end: usize,
) -> (usize, usize) {
    let page_count = page_count_for_rows(line_count, latest_rows, scrolled_rows).max(1);
    let page_from_end = page_from_end.min(page_count - 1);
    if page_from_end == 0 {
        return (line_count.saturating_sub(latest_rows), line_count);
    }
    let end = line_count
        .saturating_sub(latest_rows)
        .saturating_sub((page_from_end - 1).saturating_mul(scrolled_rows));
    (end.saturating_sub(scrolled_rows), end)
}

fn lines(state: &InteractiveState, width: usize, color_enabled: bool) -> Vec<String> {
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
                wrap_terminal_text(&super::sanitize_terminal_text(source_line), body_width)
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
