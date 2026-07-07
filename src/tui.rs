use crate::app::AppError;
use crate::{ledger, model, observability, paths};

const DEFAULT_WIDTH: usize = 92;
const MIN_WIDTH: usize = 64;
const MAX_WIDTH: usize = 120;

pub fn overview_report() -> Result<String, AppError> {
    let width = terminal_width();
    let store = observability::status()?;
    let models = observability::model_summaries()?;
    let sessions = observability::session_history(5)?;
    let identity = ledger::current_identity();

    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - overview");
    push_kv(&mut lines, width, "project", &identity.project_root);
    push_kv(&mut lines, width, "session", &identity.session_id);
    push_kv(&mut lines, width, "mode", "read-only dashboard");
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "runtime");
    push_kv(
        &mut lines,
        width,
        "observability",
        &store.path.display().to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "ledger events",
        &store.ledger_events.to_string(),
    );
    push_kv(&mut lines, width, "sessions", &store.sessions.to_string());
    push_kv(&mut lines, width, "workflows", &store.workflows.to_string());
    push_kv(
        &mut lines,
        width,
        "raw prompt/source",
        "disabled by default",
    );
    if let Some(path) = store.recovered_from {
        push_kv(
            &mut lines,
            width,
            "recovered db",
            &path.display().to_string(),
        );
    }
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "model/token summary");
    if models.is_empty() {
        push_kv(
            &mut lines,
            width,
            "model runs",
            &format!("none; candidates {}", model::candidate_summary()),
        );
    } else {
        for summary in models.iter().take(4) {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | runs {} | tokens {} | avg latency {}",
                    summary.model_id,
                    summary.runs,
                    summary.total_tokens,
                    latency_label(summary.avg_latency_ms)
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "recent sessions");
    if sessions.is_empty() {
        push_kv(&mut lines, width, "history", "none");
    } else {
        for session in sessions.iter().take(3) {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | events {} | last {}",
                    short_id(&session.session_id),
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
        "views",
        "rpotato tui | rpotato tui monitor | rpotato tui sessions",
    );
    push_footer(&mut lines, width);
    Ok(lines.join("\n"))
}

pub fn monitor_report() -> Result<String, AppError> {
    let width = terminal_width();
    let store = observability::status()?;
    let models = observability::model_summaries()?;

    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - monitor");
    push_kv(
        &mut lines,
        width,
        "observability",
        &store.path.display().to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "schema",
        &format!("v{}", store.migration_version),
    );
    push_kv(
        &mut lines,
        width,
        "model runs",
        &store.model_runs.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "token records",
        &store.token_records.to_string(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "models");
    if models.is_empty() {
        push_wrapped(
            &mut lines,
            width,
            &format!(
                "No recorded model runs yet. Candidate state: {}",
                model::candidate_summary()
            ),
        );
    } else {
        push_wrapped(
            &mut lines,
            width,
            "model | runs | prompt | completion | total | avg latency",
        );
        for summary in &models {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {} | {} | {} | {}",
                    summary.model_id,
                    summary.runs,
                    summary.prompt_tokens,
                    summary.completion_tokens,
                    summary.total_tokens,
                    latency_label(summary.avg_latency_ms)
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "actions",
        "read-only; export/prune remain monitor CLI commands",
    );
    push_footer(&mut lines, width);
    Ok(lines.join("\n"))
}

pub fn sessions_report() -> Result<String, AppError> {
    let width = terminal_width();
    let identity = ledger::current_identity();
    let sessions = observability::session_history(10)?;

    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - sessions");
    push_kv(&mut lines, width, "project", &identity.project_root);
    push_kv(&mut lines, width, "current session", &identity.session_id);
    push_rule(&mut lines, width);
    if sessions.is_empty() {
        push_wrapped(
            &mut lines,
            width,
            "No session history yet. Start with `rpotato init` or `rpotato session new`.",
        );
    } else {
        push_wrapped(&mut lines, width, "session id | events | last summary");
        for session in &sessions {
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
        "state",
        &paths::current_state_file().display().to_string(),
    );
    push_footer(&mut lines, width);
    Ok(lines.join("\n"))
}

fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH)
}

fn push_header(lines: &mut Vec<String>, width: usize, title: &str) {
    push_border(lines, width, '=');
    push_center(lines, width, title);
    push_border(lines, width, '=');
}

fn push_footer(lines: &mut Vec<String>, width: usize) {
    push_border(lines, width, '=');
    push_wrapped(
        lines,
        width,
        "beta boundary: this TUI surface reads runtime state only and does not approve, apply, resume, cancel, or mutate workflows.",
    );
}

fn push_section(lines: &mut Vec<String>, width: usize, label: &str) {
    push_wrapped(lines, width, &format!("[{label}]"));
}

fn push_rule(lines: &mut Vec<String>, width: usize) {
    push_border(lines, width, '-');
}

fn push_border(lines: &mut Vec<String>, width: usize, ch: char) {
    lines.push(ch.to_string().repeat(width));
}

fn push_center(lines: &mut Vec<String>, width: usize, value: &str) {
    let value = truncate(value, width);
    let padding = width.saturating_sub(value.len()) / 2;
    lines.push(format!("{}{}", " ".repeat(padding), value));
}

fn push_kv(lines: &mut Vec<String>, width: usize, key: &str, value: &str) {
    push_wrapped(lines, width, &format!("{key}: {value}"));
}

fn push_wrapped(lines: &mut Vec<String>, width: usize, value: &str) {
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

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let prefix = value.chars().take(width - 3).collect::<String>();
    format!("{prefix}...")
}

fn latency_label(value: Option<f64>) -> String {
    value
        .map(|latency| format!("{latency:.1}ms"))
        .unwrap_or_else(|| "not recorded".to_string())
}

fn short_id(value: &str) -> String {
    if value.len() <= 18 {
        return value.to_string();
    }
    format!("{}...", &value[..18])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_renders_read_only_dashboard() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("rpotato-tui-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("COLUMNS", "72");

        let report = overview_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("COLUMNS");

        assert!(report.contains("rpotato TUI beta - overview"));
        assert!(report.contains("mode: read-only dashboard"));
        assert!(report.contains("[runtime]"));
        assert!(report.contains("beta boundary"));
    }

    #[test]
    fn monitor_renders_model_section() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-tui-monitor-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = monitor_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(report.contains("rpotato TUI beta - monitor"));
        assert!(report.contains("[models]"));
        assert!(report.contains("No recorded model runs yet"));
    }

    #[test]
    fn sessions_renders_resume_hint() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-tui-sessions-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = sessions_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(report.contains("rpotato TUI beta - sessions"));
        assert!(report.contains("resume: rpotato session resume <session-id>"));
    }
}
