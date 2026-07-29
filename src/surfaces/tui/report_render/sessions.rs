use super::super::render::{push_footer, push_header, push_kv, push_rule, push_wrapped};
use super::super::view_model::SessionsReportView;

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
