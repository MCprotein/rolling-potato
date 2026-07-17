use super::runtime_bridge::TuiReadPage;
use super::view_model::{EvidenceReportView, InteractiveState, SessionsReportView};

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

pub(crate) fn canonical_page_report(page: TuiReadPage) -> String {
    let width = terminal_width();
    let literal_content = page.title == "diff";
    let mut lines = Vec::new();
    push_header(
        &mut lines,
        width,
        &format!("rpotato TUI beta - {}", page.title),
    );
    push_kv(&mut lines, width, "page", &(page.page + 1).to_string());
    push_kv(&mut lines, width, "freshness", page.freshness.as_str());
    push_kv(
        &mut lines,
        width,
        "continuation",
        page.continuation.as_str(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "canonical authority");
    push_kv(
        &mut lines,
        width,
        "current",
        &authority_pair(
            page.authority.current_revision,
            page.authority.current_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "workflow",
        &authority_pair(
            page.authority.workflow_revision,
            page.authority.workflow_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "ledger",
        &authority_pair(
            page.authority.ledger_sequence,
            page.authority.ledger_hash.as_deref(),
        ),
    );
    push_kv(
        &mut lines,
        width,
        "content hash",
        page.authority
            .content_hash
            .as_deref()
            .unwrap_or("unavailable"),
    );
    push_kv(
        &mut lines,
        width,
        "validated at ms",
        &page
            .authority
            .validated_at_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "content");
    if page.lines.is_empty() {
        push_wrapped(&mut lines, width, "No canonical records are available.");
    } else {
        for (index, line) in page.lines.iter().enumerate() {
            if literal_content && index > 0 {
                push_literal_block(&mut lines, width, line);
            } else {
                push_wrapped(&mut lines, width, line);
            }
        }
    }
    push_footer(&mut lines, width);
    lines.join("\n")
}

pub(crate) fn render_evidence_report(width: usize, view: &EvidenceReportView) -> String {
    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - evidence");
    push_kv(&mut lines, width, "project", &view.project_root);
    push_kv(&mut lines, width, "session", &view.session_id);
    push_kv(&mut lines, width, "mode", "read-only evidence status");
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "stores");
    push_kv(
        &mut lines,
        width,
        "runtime evidence",
        &view.runtime_evidence_file,
    );
    push_kv(
        &mut lines,
        width,
        "runtime records",
        &view.runtime_evidence_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "project evidence",
        &view.project_evidence_dir,
    );
    push_kv(
        &mut lines,
        width,
        "project artifacts",
        &view.project_artifacts.to_string(),
    );
    push_kv(&mut lines, width, "observability", &view.observability_path);
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "stop gate boundary");
    push_kv(
        &mut lines,
        width,
        "recorded evidence",
        &view.evidence_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "stop gate results",
        &view.stop_gate_results.to_string(),
    );
    push_kv(&mut lines, width, "stale policy", &view.stale_policy);
    push_kv(
        &mut lines,
        width,
        "terminal gate",
        "not implemented; this view does not pass or fail workflows",
    );
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "validate",
        "rpotato evidence validate <artifact-pointer>",
    );
    push_kv(
        &mut lines,
        width,
        "raw prompt/source",
        "disabled by default",
    );
    push_footer(&mut lines, width);
    lines.join("\n")
}

pub(crate) fn render_sessions_report(width: usize, view: &SessionsReportView) -> String {
    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - sessions");
    push_kv(&mut lines, width, "project", &view.project_root);
    push_kv(
        &mut lines,
        width,
        "current session",
        &view.current_session_id,
    );
    push_rule(&mut lines, width);
    if view.sessions.is_empty() {
        push_wrapped(
            &mut lines,
            width,
            "No session history yet. Start with `rpotato init` or `rpotato session new`.",
        );
    } else {
        push_wrapped(&mut lines, width, "session id | events | last summary");
        for session in &view.sessions {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {}",
                    session.session_id,
                    session.event_count,
                    session
                        .last_summary
                        .as_deref()
                        .unwrap_or("no summary recorded")
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "resume",
        "rpotato session resume <session-id>",
    );
    push_kv(
        &mut lines,
        width,
        "inspect",
        "rpotato tui transcript <session-id>",
    );
    push_kv(&mut lines, width, "state", &view.state_path);
    push_footer(&mut lines, width);
    lines.join("\n")
}

fn authority_pair(revision: Option<u64>, hash: Option<&str>) -> String {
    match (revision, hash) {
        (Some(revision), Some(hash)) => format!("revision={revision} hash={hash}"),
        _ => "unavailable".to_string(),
    }
}
