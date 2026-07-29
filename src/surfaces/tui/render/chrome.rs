//! Conversation chrome: welcome/header, attachments, composer, and cursor placement.

use super::{
    display_cell_width, paint, sanitize_terminal_text, truncate_chars, ACCENT_COLOR, BRAND_COLOR,
    MUTED_COLOR,
};
use crate::surfaces::tui::runtime_bridge::{TuiAttachment, TuiStatusSnapshot};

pub(super) fn render_composer(
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

pub(super) fn render_welcome(
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

pub(super) fn render_identity_header(output: &mut String, width: usize, color: bool) {
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
