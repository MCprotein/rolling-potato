use super::*;
use crate::adapters::filesystem::layout as paths;
use crate::adapters::terminal::native::{ScriptedTerminal, TerminalFault};
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::patch;
use crate::surfaces::tui::controller::{consume_outcome, run_controller};
use crate::surfaces::tui::outcome::verification_credential_issued;
use crate::surfaces::tui::render::{render_interactive_frame, sanitize_terminal_text};
use crate::surfaces::tui::runtime_bridge::{OneShotSecret, TuiFreshness, TuiReadContinuation};
use crate::surfaces::tui::view_model::{InteractiveState, InteractiveView};

#[test]
fn interactive_view_change_resets_page_and_updates_notice() {
    let mut state = InteractiveState {
        view: InteractiveView::Sessions,
        page: 4,
        selected_id: Some("workflow-selected".to_string()),
        notice: "old notice".to_string(),
    };

    state.set_view(InteractiveView::Transcript("session-next".to_string()));

    assert_eq!(
        state.view,
        InteractiveView::Transcript("session-next".to_string())
    );
    assert_eq!(state.page, 0);
    assert_eq!(state.selected_id.as_deref(), Some("workflow-selected"));
    assert_eq!(state.notice, "화면을 변경했습니다.");
}

#[test]
fn interactive_view_builds_bounded_read_request_from_viewport() {
    let state = InteractiveState {
        view: InteractiveView::ToolOutput("artifact-one".to_string()),
        page: 3,
        selected_id: None,
        notice: String::new(),
    };

    let request = state.read_request(10, 8);

    assert_eq!(
        request,
        TuiReadRequest::ToolOutput {
            artifact_id: "artifact-one".to_string(),
            page: 3,
            budget: TuiReadBudget::bounded(2, 20),
        }
    );
}

#[test]
fn one_shot_outcome_writes_secret_once_without_storing_it_in_notice() {
    let intent_id = "intent-one-shot-test";
    let secret = "ab".repeat(32);
    let outcome =
        verification_credential_issued(intent_id, OneShotSecret::new(secret.clone()).unwrap())
            .unwrap();
    let mut terminal = ScriptedTerminal::new([]);

    let notice = consume_outcome(&mut terminal, intent_id, outcome).unwrap();

    assert_eq!(terminal.frames.len(), 3);
    let rendered = terminal.frames.concat();
    assert_eq!(
        rendered.matches(&secret).count(),
        1,
        "credential must be written exactly once"
    );
    assert!(notice.was_dispatched);
    assert!(!notice.notice.contains(&secret));
    assert!(notice.notice.contains("verification.credential-issued"));
}

#[test]
fn ordinary_line_read_failure_has_a_distinct_non_secret_taxonomy() {
    let error = terminal_fault_error(TerminalFault::LineRead);

    assert!(error.message.contains("terminal.capability.mode-read"));
    assert!(!error.message.contains("terminal.secret-read.failed"));
}

#[test]
fn live_controller_compile_time_boundary_uses_only_runtime_and_terminal_authority() {
    let live = include_str!("../../surfaces/tui/controller.rs");
    for forbidden in [
        "use crate::runtime;",
        "crate::runtime::",
        "use crate::approval",
        "use crate::{evidence",
        "ledger::",
        "observability::",
        "patch::",
        "state::",
    ] {
        assert!(
            !live.contains(forbidden),
            "live boundary escaped via {forbidden}"
        );
    }
    assert!(live.contains("runtime.read_tui_page(request)"));
    assert!(live.contains("runtime.dispatch_tui_intent"));
    assert!(live.contains("trait TuiRuntimePort"));
}

#[test]
fn one_shot_approval_and_diff_views_use_the_canonical_runtime_facade() {
    let composition = include_str!("report_composition.rs");

    assert!(composition.contains("canonical_read_page(TuiReadRequest::Approvals"));
    assert!(composition.contains("canonical_read_page(TuiReadRequest::Diff"));
    assert!(!composition.contains("proposal_summaries("));
    assert!(!composition.contains("request_summaries("));
    assert!(!composition.contains("proposal_detail("));
}

#[test]
fn echo_restore_failure_exits_without_retrying_secret_input() {
    let error = terminal_fault_error(TerminalFault::EchoRestore);

    assert!(error.message.contains("terminal.echo-restore.failed"));
    assert!(error.message.contains("재시도하지 않고 TUI를 종료"));
    assert!(error.message.contains("stty echo"));
    assert!(!error.message.contains("terminal.secret-read.failed"));
}

#[test]
fn interactive_controller_exits_cleanly_and_never_emits_terminal_injection() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-interactive-controller-test");
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new(["help", "quit"]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(root);
    assert!(terminal.frames.len() >= 2);
    assert!(terminal
        .frames
        .iter()
        .all(|frame| !frame.contains('\u{001b}')));
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("rpotato>")));
}

#[test]
fn interactive_sanitizer_escapes_ansi_osc_and_control_bytes() {
    let hostile = "safe\u{001b}[31mred\u{001b}[0m\u{001b}]0;title\u{0007}\nnext\u{0000}";
    let sanitized = sanitize_terminal_text(hostile);

    assert_eq!(sanitized, "safe<esc>red<esc><esc><lf>next<ctl>");
    assert!(!sanitized.contains('\u{001b}'));
    assert!(!sanitized.contains('\u{0000}'));
}

#[test]
fn exact_outcome_notice_preserves_trusted_multiline_structure() {
    let state = InteractiveState {
        view: InteractiveView::Overview,
        page: 0,
        selected_id: None,
        notice: "결과 제목\n- code: exact.test\n- 동작: 상태를 변경하지 않았습니다.".to_string(),
    };
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: Vec::new(),
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Complete,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame(&state, &page, 120, 40);

    assert!(frame.contains(
        "notice: 결과 제목\n        - code: exact.test\n        - 동작: 상태를 변경하지 않았습니다.\n"
    ));
    assert!(!frame.contains("<lf>"));
}

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
        token_usage_complete: true,
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

    let session = crate::app::workflow_adapter::state::session_new_report().unwrap();
    let session_id = report_value(&session, "session id").unwrap();
    crate::app::workflow_adapter::state::record_event(
        "test.first",
        "first transcript event",
        "details one",
    )
    .unwrap();
    crate::app::workflow_adapter::state::record_event(
        "test.second",
        "second transcript event",
        "details two",
    )
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
    crate::app::workflow_adapter::state::initialize().unwrap();

    let report = approvals_report().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");

    assert!(report.contains("rpotato TUI beta - approvals"));
    assert!(report.contains("No canonical records are available."));
    assert!(report.contains("continuation: complete"));
}

#[test]
fn one_shot_views_do_not_admit_unbound_directory_only_proposals() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-tui-diff-test");
    let project_root = root.join("project");
    std::fs::create_dir_all(project_root.join("src")).unwrap();
    std::fs::write(project_root.join("src/lib.rs"), "pub const X: i32 = 1;\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    crate::app::workflow_adapter::state::initialize().unwrap();

    let preview = patch::preview_report("src/lib.rs", "1", "2").unwrap();
    let proposal_id = report_value(&preview, "proposal id").unwrap();
    let approvals = approvals_report().unwrap();
    let diff = diff_report(&proposal_id).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");

    assert!(!approvals.contains(&proposal_id));
    assert!(approvals.contains("No canonical records are available."));
    assert!(diff.contains("rpotato TUI beta - diff"));
    assert!(diff.contains("continuation: unavailable"));
    assert!(diff.contains("active workflow canonical binding이 없습니다."));
    assert!(!diff.contains("-pub const X: i32 = 1;"));
    assert!(!diff.contains("+pub const X: i32 = 2;"));
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
    crate::app::workflow_adapter::state::initialize().unwrap();

    crate::app::observability_adapter::record_resource_sample(
        &crate::app::observability_adapter::ResourceSampleMetric {
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
        },
    )
    .unwrap();
    let err = crate::app::collaboration_adapter::team::admission_report(
        2,
        &["README.md".to_string()],
        &[],
        &[],
    )
    .unwrap_err();
    let report = approvals_report().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");

    assert!(err.message.contains("approval request: team-event-"));
    assert!(report.contains("team-admission"));
    assert!(report.contains("pending-approval"), "{report}");
    assert!(report.contains("canonical-event="));
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
