use super::*;
use crate::adapters::filesystem::layout as paths;
use crate::adapters::terminal::native::{ScriptedTerminal, TerminalFault};
use crate::surfaces::tui::controller::{consume_outcome, run_controller};
use crate::surfaces::tui::outcome::verification_credential_issued;
use crate::surfaces::tui::render::{
    render_interactive_frame, render_interactive_frame_with_options, sanitize_terminal_text,
};
use crate::surfaces::tui::runtime_bridge::{
    OneShotSecret, TuiFreshness, TuiReadBudget, TuiReadContinuation,
};
use crate::surfaces::tui::runtime_bridge::{TuiBackendStatus, TuiStatusSnapshot};
use crate::surfaces::tui::view_model::{InteractiveState, InteractiveView};

#[test]
fn interactive_view_change_resets_page_and_updates_notice() {
    let mut state = InteractiveState {
        view: InteractiveView::Sessions,
        page: 4,
        selected_id: Some("workflow-selected".to_string()),
        notice: "old notice".to_string(),
        notice_page: 3,
    };

    state.set_view(InteractiveView::Transcript("session-next".to_string()));

    assert_eq!(
        state.view,
        InteractiveView::Transcript("session-next".to_string())
    );
    assert_eq!(state.page, 0);
    assert_eq!(state.selected_id.as_deref(), Some("workflow-selected"));
    assert_eq!(state.notice, "화면을 변경했습니다.");
    assert_eq!(state.notice_page, 0);
}

#[test]
fn interactive_view_builds_bounded_read_request_from_viewport() {
    let state = InteractiveState {
        view: InteractiveView::ToolOutput("artifact-one".to_string()),
        page: 3,
        selected_id: None,
        notice: String::new(),
        notice_page: 0,
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
    assert!(live.contains("runtime.submit_request"));
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
    std::env::set_var("RPOTATO_TEST_SKIP_UPDATE_CHECK", "1");
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new(["/model", "/help", "/compact", "/quit"]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_TEST_SKIP_UPDATE_CHECK");
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
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("gemma-4-e4b")));
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("context 131k")));
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("16 GB 적합성은 미확정")));
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("/compact: 현재 대화 컨텍스트 압축")));
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("context compact 결과")));
}

#[test]
fn interactive_controller_notifies_and_applies_update_without_leaving_tui() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-interactive-update-test");
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var(
        "RPOTATO_TEST_LATEST_RELEASE_JSON",
        r#"{"tag_name":"v9.0.0"}"#,
    );
    std::env::set_var(
        "RPOTATO_TEST_UPDATE_REPORT",
        "rpotato update\n- status: updated\n- installed: v9.0.0",
    );
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new(["/update", "yes", "/quit"]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_TEST_LATEST_RELEASE_JSON");
    std::env::remove_var("RPOTATO_TEST_UPDATE_REPORT");
    let _ = std::fs::remove_dir_all(root);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("새 rpotato 버전이 있습니다"));
    assert!(rendered.contains("/update 를 입력하면"));
    assert!(rendered.contains("SHA-256 검증"));
    assert!(rendered.contains("status: updated"));
    assert!(rendered.contains("installed: v9.0.0"));
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
        notice_page: 0,
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
fn interactive_status_bar_uses_real_metric_labels_below_the_ansi_input_line() {
    let state = InteractiveState::new();
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
    let mut status = TuiStatusSnapshot {
        model: "gemma-4-e4b".to_string(),
        context_tokens_used: Some(1024),
        context_limit_tokens: Some(4096),
        has_compaction_checkpoint: false,
        backend: TuiBackendStatus::Ready,
        session_id: "session-long-identifier".to_string(),
    };

    let frame = render_interactive_frame_with_options(&state, &page, &status, 120, 40, true, true);

    let prompt = frame.find("rpotato> ").unwrap();
    let status_line = frame.find("model gemma-4-e4b").unwrap();
    assert!(prompt < status_line);
    assert!(frame.contains("ctx 1024/4096 (25%)"));
    assert!(frame.contains("compact auto@75%"));
    assert!(frame.contains("backend ready"));
    assert!(frame.contains("\u{001b}[32m"));
    assert!(frame.ends_with("\u{001b}[2A\r\u{001b}[9C"));

    status.has_compaction_checkpoint = true;
    let saved = render_interactive_frame_with_options(&state, &page, &status, 120, 40, true, true);
    assert!(saved.contains("compact saved"));

    status.has_compaction_checkpoint = false;
    status.context_tokens_used = Some(3072);
    let due = render_interactive_frame_with_options(&state, &page, &status, 120, 40, true, true);
    assert!(due.contains("compact due"));
}

#[test]
fn no_color_forces_a_plain_frame_without_layout_escape_sequences() {
    let state = InteractiveState::new();
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: vec!["body".to_string()],
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Complete,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame_with_options(
        &state,
        &page,
        &TuiStatusSnapshot::unavailable(),
        80,
        24,
        true,
        false,
    );

    assert!(!frame.contains('\u{001b}'));
    assert!(frame.contains("backend unavailable"));
}

#[test]
fn long_notice_keeps_composer_and_status_inside_the_terminal_row_budget() {
    let mut state = InteractiveState::new();
    state.notice = (0..20)
        .map(|index| format!("notice line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let page = TuiReadPage {
        title: "overview".to_string(),
        lines: (0..20).map(|index| format!("body {index}")).collect(),
        page: 0,
        has_previous: false,
        has_next: false,
        freshness: TuiFreshness::Fresh,
        continuation: TuiReadContinuation::Complete,
        authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
    };

    let frame = render_interactive_frame_with_options(
        &state,
        &page,
        &TuiStatusSnapshot::unavailable(),
        80,
        10,
        true,
        true,
    );

    assert!(frame.find("rpotato> ").unwrap() < frame.find("model 미선택").unwrap());
    assert!(frame.matches('\n').count() < 10);
    assert!(frame.contains("…"));
    assert!(frame.ends_with("\u{001b}[2A\r\u{001b}[9C"));
}

#[test]
fn long_notice_pages_preserve_later_response_lines() {
    let mut state = InteractiveState::new();
    state.notice = (0..20)
        .map(|index| format!("response line {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    for _ in 0..6 {
        state.next_notice_page(10);
    }
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

    let frame = render_interactive_frame(&state, &page, 80, 10);

    assert!(frame.contains("response line 18"));
    assert!(frame.contains("response line 19"));
    assert!(!frame.contains("response line 0"));
    state.previous_notice_page();
    assert_eq!(state.notice_page, 5);
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
