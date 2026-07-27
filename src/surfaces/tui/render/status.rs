//! Compact model, context, backend, vision, and session status presentation.

use super::{
    display_cell_width, paint, sanitize_terminal_text, truncate_chars, ACCENT_COLOR, FAILED_COLOR,
    HEALTHY_COLOR, MUTED_COLOR, WARNING_COLOR,
};
use crate::surfaces::tui::runtime_bridge::{TuiBackendStatus, TuiStatusSnapshot, TuiVisionStatus};

pub(super) fn render_status_line(
    status: &TuiStatusSnapshot,
    context_estimate: Option<u32>,
    width: usize,
    color: bool,
) -> String {
    let estimated = context_estimate.is_some();
    let used = context_estimate.or(status.context_tokens_used);
    let (context, percent) = match (used, status.context_limit_tokens) {
        (Some(used), Some(limit)) if limit > 0 => {
            let percent = used.saturating_mul(100) / limit;
            (
                format!(
                    "ctx {}{used}/{limit} ({percent}%)",
                    if estimated { "~" } else { "" }
                ),
                Some(percent),
            )
        }
        (Some(used), _) => (
            format!("ctx {}{used}/—", if estimated { "~" } else { "" }),
            None,
        ),
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
        (format!("local {}", status.backend.as_str()), backend_color),
        (
            format!("vision {}", status.vision.as_str()),
            match status.vision {
                TuiVisionStatus::Ready => HEALTHY_COLOR,
                TuiVisionStatus::OnDemand => ACCENT_COLOR,
                TuiVisionStatus::Unsupported | TuiVisionStatus::Unavailable => MUTED_COLOR,
            },
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

fn short_status_id(value: &str) -> String {
    if value.chars().count() <= 12 {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(11).collect::<String>())
    }
}
