use crate::app::AppError;
use crate::approval;
use crate::{evidence, ledger, model, observability, patch, paths};

const DEFAULT_WIDTH: usize = 92;
const MIN_WIDTH: usize = 64;
const MAX_WIDTH: usize = 120;

pub fn overview_report() -> Result<String, AppError> {
    let width = terminal_width();
    let store = observability::status()?;
    let models = observability::model_summaries()?;
    let sessions = observability::session_history(5)?;
    let identity = ledger::validated_current_identity()?;

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
                    "{} | runs {} | tokens {} | avg latency {} | avg tps {}",
                    summary.model_id,
                    summary.runs,
                    summary.total_tokens,
                    latency_label(summary.avg_latency_ms),
                    tps_label(summary.avg_tokens_per_second)
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
        "rpotato tui | rpotato tui monitor | rpotato tui sessions | rpotato tui transcript <session-id> | rpotato tui approvals | rpotato tui evidence",
    );
    push_footer(&mut lines, width);
    Ok(lines.join("\n"))
}

pub fn monitor_report() -> Result<String, AppError> {
    let width = terminal_width();
    let store = observability::status()?;
    let models = observability::model_summaries()?;
    let resource = observability::latest_resource_sample()?;

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
    push_kv(
        &mut lines,
        width,
        "resource samples",
        &store.resource_samples.to_string(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "resource pressure");
    if let Some(sample) = resource {
        push_wrapped(
            &mut lines,
            width,
            &format!(
                "pressure: {} | backend: {} | pid: {} | sample count: {} | recorded ms: {}",
                sample.pressure_status,
                sample.backend_id,
                sample.pid,
                sample.sample_count,
                sample.recorded_at_ms
            ),
        );
        push_wrapped(
            &mut lines,
            width,
            &format!(
                "cpu: {} | avg rss: {}",
                percent_label(sample.process_cpu_percent),
                bytes_label(sample.average_rss_bytes)
            ),
        );
        push_wrapped(
            &mut lines,
            width,
            &format!(
                "peak rss: {} | disk: {}",
                bytes_label(sample.peak_rss_bytes),
                bytes_label(sample.disk_bytes)
            ),
        );
        push_wrapped(
            &mut lines,
            width,
            &format!("latest sample: {}", short_id(&sample.resource_sample_id)),
        );
    } else {
        push_wrapped(
            &mut lines,
            width,
            "No resource samples yet. Run backend start, backend status, or backend chat after a sidecar is running.",
        );
    }
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
            "model | runs | prompt | completion | total | avg ms | tps",
        );
        for summary in &models {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {} | {} | {} | {} | {}",
                    summary.model_id,
                    summary.runs,
                    summary.prompt_tokens,
                    summary.completion_tokens,
                    summary.total_tokens,
                    latency_label(summary.avg_latency_ms),
                    tps_label(summary.avg_tokens_per_second)
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
    let identity = ledger::validated_current_identity()?;
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
        "inspect",
        "rpotato tui transcript <session-id>",
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

pub fn transcript_report(session_id: &str) -> Result<String, AppError> {
    let width = terminal_width();
    let session = observability::session_entry(session_id)?.ok_or_else(|| {
        AppError::blocked(format!(
            "tui transcript 차단\n- session id: {}\n- 이유: 현재 project의 session history에서 찾지 못했습니다.\n- 확인: rpotato tui sessions",
            session_id
        ))
    })?;
    let events = observability::session_events(session_id, 40)?;

    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - transcript");
    push_kv(&mut lines, width, "project", &session.project_root);
    push_kv(&mut lines, width, "session", &session.session_id);
    push_kv(
        &mut lines,
        width,
        "started",
        &session.started_at_ms.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "last event",
        &session
            .last_event_at_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
    );
    push_kv(
        &mut lines,
        width,
        "events",
        &session.event_count.to_string(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "timeline");
    if events.is_empty() {
        push_wrapped(
            &mut lines,
            width,
            "No ledger events are projected for this session yet.",
        );
    } else {
        push_wrapped(&mut lines, width, "ts_ms | event type | event id | summary");
        for event in &events {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {} | {}",
                    event.ts_ms,
                    event.event_type,
                    short_id(&event.event_id),
                    event.summary
                ),
            );
        }
        if session.event_count > i64::try_from(events.len()).unwrap_or(i64::MAX) {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "showing first {} projected events; total event count is {}",
                    events.len(),
                    session.event_count
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "resume",
        &format!("rpotato session resume {}", session.session_id),
    );
    push_kv(
        &mut lines,
        width,
        "raw details",
        "not shown in the TUI beta by default",
    );
    push_footer(&mut lines, width);
    Ok(lines.join("\n"))
}

pub fn approvals_report() -> Result<String, AppError> {
    let width = terminal_width();
    let proposals = patch::proposal_summaries(12)?;
    let requests = approval::request_summaries(12)?;

    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - approvals");
    push_kv(
        &mut lines,
        width,
        "proposal dir",
        &paths::project_patch_proposals_dir().display().to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "request dir",
        &paths::project_approval_requests_dir().display().to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "records",
        &(proposals.len() + requests.len()).to_string(),
    );
    push_rule(&mut lines, width);
    if proposals.is_empty() && requests.is_empty() {
        push_wrapped(
            &mut lines,
            width,
            "No approval records yet. Create one with `rpotato patch preview --path <path> --find <text> --replace <text>` or a blocking `rpotato team admit` preflight.",
        );
    } else {
        push_wrapped(
            &mut lines,
            width,
            "source | status | id | target/reason | items",
        );
        for request in &requests {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "{} | {} | {} | {} | {}",
                    request.source,
                    request.status,
                    request.request_id,
                    request.reason,
                    request.item_count
                ),
            );
        }
        for proposal in &proposals {
            push_wrapped(
                &mut lines,
                width,
                &format!(
                    "patch | {} | {} | {} | {}",
                    proposal.status,
                    proposal.proposal_id,
                    proposal.relative_path,
                    proposal.replacements
                ),
            );
        }
    }
    push_rule(&mut lines, width);
    push_kv(
        &mut lines,
        width,
        "inspect",
        "patch rows: rpotato tui diff <proposal-id>; team rows: inspect approval request record",
    );
    push_kv(
        &mut lines,
        width,
        "apply",
        "use rpotato patch approve outside the TUI after reviewing the diff",
    );
    push_footer(&mut lines, width);
    Ok(lines.join("\n"))
}

pub fn diff_report(proposal_id: &str) -> Result<String, AppError> {
    let width = terminal_width();
    let detail = patch::proposal_detail(proposal_id)?;

    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - diff");
    push_kv(
        &mut lines,
        width,
        "proposal id",
        &detail.summary.proposal_id,
    );
    push_kv(&mut lines, width, "status", &detail.summary.status);
    push_kv(&mut lines, width, "path", &detail.summary.relative_path);
    push_kv(
        &mut lines,
        width,
        "replacements",
        &detail.summary.replacements,
    );
    push_kv(
        &mut lines,
        width,
        "original sha256",
        &detail.summary.original_sha256,
    );
    push_kv(
        &mut lines,
        width,
        "proposed sha256",
        &detail.summary.proposed_sha256,
    );
    push_kv(
        &mut lines,
        width,
        "approval",
        "최초 proposal 출력에서 발급된 token을 사용하세요; 상태/TUI에는 hash만 남아 token을 재구성할 수 없습니다.",
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "diff");
    push_literal_block(&mut lines, width, &detail.diff);
    push_rule(&mut lines, width);
    push_kv(&mut lines, width, "token display", "unavailable by design");
    push_footer(&mut lines, width);
    Ok(lines.join("\n"))
}

pub fn evidence_report() -> Result<String, AppError> {
    let width = terminal_width();
    let identity = ledger::validated_current_identity()?;
    let store = observability::status()?;
    let evidence = evidence::store_status()?;

    let mut lines = Vec::new();
    push_header(&mut lines, width, "rpotato TUI beta - evidence");
    push_kv(&mut lines, width, "project", &identity.project_root);
    push_kv(&mut lines, width, "session", &identity.session_id);
    push_kv(&mut lines, width, "mode", "read-only evidence status");
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "stores");
    push_kv(
        &mut lines,
        width,
        "runtime evidence",
        &evidence.runtime_evidence_file.display().to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "runtime records",
        &evidence.runtime_evidence_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "project evidence",
        &evidence.project_evidence_dir.display().to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "project artifacts",
        &evidence.project_artifacts.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "observability",
        &store.path.display().to_string(),
    );
    push_rule(&mut lines, width);
    push_section(&mut lines, width, "stop gate boundary");
    push_kv(
        &mut lines,
        width,
        "recorded evidence",
        &store.evidence_records.to_string(),
    );
    push_kv(
        &mut lines,
        width,
        "stop gate results",
        &store.stop_gate_results.to_string(),
    );
    push_kv(&mut lines, width, "stale policy", evidence.stale_policy);
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

fn push_literal_block(lines: &mut Vec<String>, width: usize, value: &str) {
    for line in value.lines() {
        lines.push(truncate(line, width));
    }
    if value.is_empty() {
        lines.push(String::new());
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

fn tps_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1} tok/s"))
        .unwrap_or_else(|| "not recorded".to_string())
}

fn percent_label(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn bytes_label(value: Option<u64>) -> String {
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
        assert!(report.contains("[resource pressure]"));
        assert!(report.contains("resource samples: 0"));
        assert!(report.contains("No resource samples yet"));
        assert!(report.contains("[models]"));
        assert!(report.contains("No recorded model runs yet"));
    }

    #[test]
    fn monitor_renders_resource_pressure_and_token_throughput() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-resource-monitor-test");
        let project_root = root.join("project");
        std::fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("COLUMNS", "64");
        let identity = ledger::validated_current_identity().unwrap();

        observability::record_model_run(&observability::ModelRunMetric {
            model_run_id: "model-run-tui-resource".to_string(),
            session_id: identity.session_id.clone(),
            workflow_id: None,
            model_id: "qwen-test".to_string(),
            model_artifact_hash: None,
            backend_id: Some("llama.cpp".to_string()),
            backend_version: None,
            quantization: None,
            context_limit_tokens: Some(4096),
            started_at_ms: 1000,
            first_token_latency_ms: Some(25.0),
            total_latency_ms: Some(200.0),
            prompt_eval_ms: None,
            generation_eval_ms: None,
            tokens_per_second: Some(12.5),
            cancelled: false,
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
            context_tokens_used: 10,
            context_tokens_dropped: 0,
            ontology_tokens: 0,
            tool_summary_tokens: 0,
            max_output_tokens: Some(64),
        })
        .unwrap();
        observability::record_resource_sample(&observability::ResourceSampleMetric {
            resource_sample_id: "resource-sample-tui-resource".to_string(),
            session_id: identity.session_id,
            backend_id: "llama.cpp".to_string(),
            pid: 12345,
            process_cpu_percent: Some(84.2),
            average_rss_bytes: Some(256 * 1024 * 1024),
            peak_rss_bytes: Some(512 * 1024 * 1024),
            disk_bytes: Some(1536),
            sample_count: 3,
            pressure_status: "degraded".to_string(),
            recorded_at_ms: 2000,
        })
        .unwrap();

        let report = monitor_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("COLUMNS");
        let _ = std::fs::remove_dir_all(root);

        assert!(report.contains("[resource pressure]"));
        assert!(report.contains("resource samples: 1"));
        assert!(report.contains("pressure: degraded"));
        assert!(report.contains("cpu: 84.2%"));
        assert!(report.contains("avg rss: 256.0 MiB"));
        assert!(report.contains("peak rss: 512.0 MiB"));
        assert!(report.contains("disk: 1.5 KiB"));
        assert!(report.contains("avg ms | tps"));
        assert!(report.contains("12.5 tok/s"));
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

    #[test]
    fn transcript_renders_session_event_timeline() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-transcript-test");
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let session = crate::state::session_new_report().unwrap();
        let session_id = report_value(&session, "session id").unwrap();
        crate::state::record_event("test.first", "first transcript event", "details one").unwrap();
        crate::state::record_event("test.second", "second transcript event", "details two")
            .unwrap();
        let report = transcript_report(&session_id).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(report.contains("rpotato TUI beta - transcript"));
        assert!(report.contains(&format!("session: {session_id}")));
        assert!(report.contains("[timeline]"));
        assert!(report.contains("test.first"));
        assert!(report.contains("first transcript event"));
        assert!(report.contains("test.second"));
        assert!(report.contains("raw details: not shown"));
    }

    #[test]
    fn approvals_renders_empty_queue() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-approvals-empty-test");
        std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = approvals_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(report.contains("rpotato TUI beta - approvals"));
        assert!(report.contains("No approval records yet"));
        assert!(report.contains("inspect: patch rows: rpotato tui diff <proposal-id>"));
    }

    #[test]
    fn diff_renders_preview_record_literal_diff() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-diff-test");
        let project_root = root.join("project");
        std::fs::create_dir_all(project_root.join("src")).unwrap();
        std::fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let preview = patch::preview_report("src/lib.rs", "1", "2").unwrap();
        let proposal_id = report_value(&preview, "proposal id").unwrap();
        let approvals = approvals_report().unwrap();
        let diff = diff_report(&proposal_id).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(approvals.contains("pending-approval"));
        assert!(approvals.contains(&proposal_id));
        assert!(diff.contains("rpotato TUI beta - diff"));
        assert!(diff.contains("-pub const X: i32 = 1;"));
        assert!(diff.contains("+pub const X: i32 = 2;"));
        assert!(diff.contains("token display: unavailable by design"));
        assert!(!diff.contains("--token "));
    }

    #[test]
    fn approvals_renders_team_admission_request() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-approvals-team-test");
        let project_root = root.join("project");
        std::fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        crate::observability::record_resource_sample(&crate::observability::ResourceSampleMetric {
            resource_sample_id: "resource-sample-tui-approvals-team".to_string(),
            session_id: "session-tui-approvals-team".to_string(),
            backend_id: "llama.cpp".to_string(),
            pid: 4242,
            process_cpu_percent: Some(12.0),
            average_rss_bytes: Some(512 * 1024 * 1024),
            peak_rss_bytes: Some(512 * 1024 * 1024),
            disk_bytes: Some(2048),
            sample_count: 1,
            pressure_status: "normal".to_string(),
            recorded_at_ms: 1234,
        })
        .unwrap();
        let err =
            crate::team::admission_report(2, &["README.md".to_string()], &[], &[]).unwrap_err();
        let report = approvals_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");

        assert!(err.message.contains("approval request: team-event-"));
        assert!(report.contains("team-admission"));
        assert!(report.contains("pending-approval"));
        assert!(report.contains("policy-blocked"));
        assert!(report.contains("source | status | id | target/reason | items"));
    }

    #[test]
    fn evidence_renders_stop_gate_status_without_mutating() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-tui-evidence-test");
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("COLUMNS", "68");

        std::fs::create_dir_all(paths::state_dir()).unwrap();
        std::fs::create_dir_all(paths::project_evidence_dir()).unwrap();
        std::fs::write(
            paths::runtime_evidence_file(),
            "{\"evidence_id\":\"one\"}\n",
        )
        .unwrap();
        std::fs::write(paths::project_evidence_dir().join("one.txt"), "one").unwrap();

        let report = evidence_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("COLUMNS");

        assert!(report.contains("rpotato TUI beta - evidence"));
        assert!(report.contains("mode: read-only evidence status"));
        assert!(report.contains("runtime records: 1"));
        assert!(report.contains("project artifacts: 1"));
        assert!(report.contains("[stop gate boundary]"));
        assert!(report.contains("terminal gate: not implemented"));
        assert!(report.contains("validate: rpotato evidence validate <artifact-pointer>"));
        assert!(report.contains("beta boundary"));
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
    }

    fn report_value(report: &str, key: &str) -> Option<String> {
        let prefix = format!("- {key}: ");
        report
            .lines()
            .find_map(|line| line.strip_prefix(&prefix).map(|value| value.to_string()))
    }
}
