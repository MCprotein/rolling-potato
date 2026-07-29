use super::*;

#[test]
fn v03713_unit_test_runtime_fixture_lives_under_test_support() {
    assert!(!Path::new("src/test_support.rs").exists());
    assert!(Path::new("tests/support/runtime_fixture.rs").is_file());

    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("#[path = \"../tests/support/runtime_fixture.rs\"]"));
    assert!(main.contains("mod test_support;"));
}

#[test]
fn v03713_tui_bridge_owns_read_and_selection_dtos() {
    let tui_adapter = "src/app/tui_adapter.rs";
    let tui_tests = "src/app/tui_adapter/tests.rs";
    let tui_report_tests = "src/app/tui_adapter/report_tests.rs";
    let tui_model_switch = "src/app/tui_adapter/model_switch.rs";
    let tui_runtime = "src/app/tui_adapter/runtime.rs";
    let tui_runtime_backend = "src/app/tui_adapter/runtime/backend.rs";
    assert!(Path::new(tui_adapter).is_file());
    assert!(Path::new(tui_tests).is_file());
    assert!(Path::new(tui_report_tests).is_file());
    assert!(Path::new(tui_model_switch).is_file());
    assert!(Path::new(tui_runtime).is_file());
    assert!(Path::new(tui_runtime_backend).is_file());
    assert!(!Path::new("src/tui.rs").exists());
    assert!(!Path::new("src/tui").exists());

    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod tui;"));
    let app_root = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod tui_adapter;"),
        "application root does not register the TUI adapter"
    );
    let bridge = fs::read_to_string("src/surfaces/tui/runtime_bridge.rs").unwrap();
    let tui_runtime_source = fs::read_to_string(tui_runtime).unwrap();
    let runtime_backend = fs::read_to_string(tui_runtime_backend).unwrap();
    assert!(tui_runtime_source
        .lines()
        .any(|line| line == "mod backend;"));
    assert!(!tui_runtime_source.contains("fn ensure_runtime_ready("));
    assert!(runtime_backend.contains("pub(super) fn ensure_runtime_ready("));
    for definition in [
        "struct TuiReadBudget",
        "enum TuiReadRequest",
        "struct TuiReadPage",
        "struct SelectionLease",
        "struct SelectionObservation",
        "enum TuiFreshness",
        "enum TuiIntent",
        "struct OneShotSecret",
        "fn new_tui_intent_id",
        "fn lease_matches_active_workflow",
        "fn lease_matches_terminal_selection",
    ] {
        assert!(
            bridge.contains(definition),
            "TUI runtime bridge is missing {definition}"
        );
    }

    let outcome_path = "src/surfaces/tui/outcome.rs";
    let outcome_oracle_path = "src/surfaces/tui/outcome/oracle.rs";
    assert!(Path::new(outcome_oracle_path).is_file());
    let outcome = fs::read_to_string(outcome_path).unwrap();
    let outcome_oracle = fs::read_to_string(outcome_oracle_path).unwrap();
    assert!(
        outcome.lines().any(|line| line == "mod oracle;"),
        "TUI outcome owner does not register its exact oracle"
    );
    for definition in [
        "enum TuiOutcomeCode",
        "struct TuiOutcome",
        "fn unsupported_source_platform_outcome",
        "fn validate_tui_id",
    ] {
        assert!(
            outcome.contains(definition),
            "TUI outcome owner is missing {definition}"
        );
    }
    for definition in [
        "pub(crate) fn exact_tui_outcome(",
        "fn required_outcome_id",
        "fn required_outcome_phase",
        "fn required_outcome_platform",
        "fn corrupt_outcome_placeholder",
    ] {
        assert!(
            outcome_oracle.contains(definition),
            "TUI outcome oracle is missing {definition}"
        );
        assert!(
            !outcome.contains(definition),
            "TUI outcome DTO owner still owns exact oracle behavior: {definition}"
        );
    }
    assert!(outcome.lines().count() < 250);
    assert!(outcome_oracle.lines().count() < 425);

    let app_runtime = fs::read_to_string("src/app/runtime_adapter.rs").unwrap();
    assert!(!app_runtime.contains("pub struct TuiReadBudget"));
    assert!(!app_runtime.contains("pub struct SelectionLease"));
    assert!(!app_runtime.contains("pub enum TuiIntent"));
    assert!(!app_runtime.contains("pub struct OneShotSecret"));
    assert!(!app_runtime.contains("pub enum TuiOutcomeCode"));
    assert!(!app_runtime.contains("pub struct TuiOutcome"));
    assert!(!app_runtime.contains("pub(crate) fn exact_tui_outcome"));
    assert!(!app_runtime.contains("fn unsupported_source_platform_outcome"));
    assert!(!app_runtime.contains("fn new_tui_intent_id"));
    assert!(!app_runtime.contains("fn tui_lease_matches_workflow_under_transition"));
    assert!(!app_runtime.contains("fn tui_lease_matches_terminal_selection_under_transition"));
    assert!(!app_runtime.contains("fn validate_tui_id"));
    assert!(!app_runtime.contains("fn tui_selection_lease"));
    assert!(!app_runtime.contains("fn tui_gate_descriptor"));
    assert!(!app_runtime.contains("fn dispatch_tui_intent"));

    for legacy_owner in [
        "src/app/patch_adapter.rs",
        "src/app/workflow_adapter/state.rs",
        tui_adapter,
    ] {
        let source = fs::read_to_string(legacy_owner).unwrap();
        for facade_type in [
            "crate::runtime::SelectionLease",
            "crate::runtime::TuiGateKind",
        ] {
            assert!(
                !source.contains(facade_type),
                "{legacy_owner} still imports TUI contract through {facade_type}"
            );
        }
    }

    let tui_read_path = "src/composition/tui_read.rs";
    let tui_read_state_path = "src/composition/tui_read/state.rs";
    let tui_read_transcript_path = "src/composition/tui_read/transcript.rs";
    let tui_read_review_path = "src/composition/tui_read/review.rs";
    let tui_read_common_path = "src/composition/tui_read/common.rs";
    let tui_read = fs::read_to_string(tui_read_path).unwrap();
    let tui_read_state = fs::read_to_string(tui_read_state_path).unwrap();
    let tui_read_transcript = fs::read_to_string(tui_read_transcript_path).unwrap();
    let tui_read_review = fs::read_to_string(tui_read_review_path).unwrap();
    let tui_read_common = fs::read_to_string(tui_read_common_path).unwrap();
    assert!(tui_read.contains("fn read_tui_page"));
    assert!(tui_read.contains("trait TuiReadPort"));
    for registration in [
        "#[path = \"tui_read/common.rs\"]",
        "#[path = \"tui_read/review.rs\"]",
        "#[path = \"tui_read/state.rs\"]",
        "#[path = \"tui_read/transcript.rs\"]",
    ] {
        assert!(
            tui_read.contains(registration),
            "TUI read facade is missing owner registration: {registration}"
        );
    }
    for (owner, definition) in [
        (&tui_read_state, "pub(super) fn overview("),
        (&tui_read_state, "pub(super) fn monitor("),
        (&tui_read_state, "pub(super) fn sessions("),
        (&tui_read_transcript, "pub(super) fn transcript("),
        (&tui_read_transcript, "pub(super) fn tool_output("),
        (&tui_read_review, "pub(super) fn approvals("),
        (&tui_read_review, "pub(super) fn diff("),
        (&tui_read_review, "pub(super) fn evidence("),
        (&tui_read_common, "pub(super) fn freshness("),
    ] {
        assert!(
            owner.contains(definition),
            "TUI read owner is missing responsibility: {definition}"
        );
        assert!(
            !tui_read.contains(definition),
            "TUI read facade still owns moved responsibility: {definition}"
        );
    }
    for (owner, line_budget) in [
        (tui_read_path, 100),
        (tui_read_common_path, 50),
        (tui_read_state_path, 225),
        (tui_read_transcript_path, 200),
        (tui_read_review_path, 225),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "TUI read owner {owner} exceeded its {line_budget}-line budget"
        );
    }
    assert!(!app_runtime.contains("fn read_tui_page"));

    let tui_action = fs::read_to_string("src/composition/tui_action.rs").unwrap();
    for definition in [
        "trait TuiActionPort",
        "enum TuiMutationFailure",
        "fn selection_lease",
        "fn gate_descriptor",
        "fn dispatch_intent",
    ] {
        assert!(
            tui_action.contains(definition),
            "TUI action owner is missing {definition}"
        );
    }

    let page = fs::read_to_string("src/surfaces/tui/page.rs").unwrap();
    for definition in [
        "fn bounded_budget_for",
        "fn page_slice",
        "fn paged_chars",
        "fn paged_diff",
        "fn page_has_next",
        "fn page_continuation",
        "fn state_page_authority",
        "fn unavailable_page",
        "fn build_page",
    ] {
        assert!(
            page.contains(definition),
            "TUI page owner is missing {definition}"
        );
        assert!(
            !app_runtime.contains(definition),
            "legacy runtime still owns {definition}"
        );
    }

    let view_model = fs::read_to_string("src/surfaces/tui/view_model.rs").unwrap();
    for definition in [
        "enum InteractiveView",
        "struct InteractiveState",
        "struct EvidenceReportView",
        "struct SessionsReportView",
        "struct SessionSummaryView",
        "struct OverviewReportView",
        "struct MonitorReportView",
        "struct TranscriptReportView",
        "fn set_view",
        "fn read_request",
    ] {
        assert!(
            view_model.contains(definition),
            "TUI view-model owner is missing {definition}"
        );
    }
    let tui_composition = fs::read_to_string(tui_adapter).unwrap();
    let tui_test_source = fs::read_to_string(tui_tests).unwrap();
    let tui_report_test_source = fs::read_to_string(tui_report_tests).unwrap();
    let model_switch = fs::read_to_string(tui_model_switch).unwrap();
    let interactive_runtime = fs::read_to_string(tui_runtime).unwrap();
    let report_composition =
        fs::read_to_string("src/app/tui_adapter/report_composition.rs").unwrap();
    assert!(tui_test_source.contains("surfaces::tui::view_model"));
    assert!(tui_composition.contains("impl TuiActionPort for TuiActionAdapter"));
    assert!(tui_composition.contains("impl TuiReadPort for TuiReadAdapter"));
    assert!(
        tui_composition
            .lines()
            .any(|line| line == "mod report_composition;"),
        "TUI adapter does not register report composition owner"
    );
    assert!(
        tui_composition.contains("#[path = \"tui_adapter/tests.rs\"]"),
        "TUI adapter does not register its regression-test owner"
    );
    assert!(
        tui_composition.contains("#[path = \"tui_adapter/report_tests.rs\"]"),
        "TUI adapter does not register its report regression owner"
    );
    for (owner, responsibility) in [
        (&model_switch, "pub(super) fn switch_prepared_model("),
        (&model_switch, "fn rollback_error("),
        (
            &interactive_runtime,
            "impl TuiRuntimePort for TuiRuntimeAdapter",
        ),
        (&runtime_backend, "fn ensure_runtime_ready("),
    ] {
        assert!(
            owner.contains(responsibility),
            "TUI extracted owner is missing {responsibility}"
        );
        assert!(
            !tui_composition.contains(responsibility),
            "TUI adapter still owns extracted responsibility: {responsibility}"
        );
    }
    for regression in [
        "fn overview_renders_read_only_dashboard(",
        "fn monitor_renders_resource_pressure_and_token_throughput(",
        "fn transcript_renders_session_event_timeline(",
    ] {
        assert!(
            tui_report_test_source.contains(regression),
            "TUI report regression owner is missing: {regression}"
        );
    }
    for regression in [
        "fn interactive_view_change_resets_page_and_updates_notice(",
        "fn one_shot_outcome_writes_secret_once_without_storing_it_in_notice(",
        "fn interactive_controller_exits_cleanly_and_never_emits_terminal_injection(",
        "fn approvals_renders_team_admission_request(",
        "fn evidence_renders_stop_gate_status_without_mutating(",
    ] {
        assert!(
            tui_test_source.contains(regression),
            "TUI regression owner is missing: {regression}"
        );
        assert!(
            !tui_composition.contains(regression),
            "TUI adapter still owns regression test: {regression}"
        );
    }
    assert!(!tui_composition.contains("enum InteractiveView"));
    assert!(!tui_composition.contains("struct InteractiveState"));
    for responsibility in [
        "pub fn overview_report(",
        "pub fn monitor_report(",
        "pub fn sessions_report(",
        "pub fn transcript_report(",
        "pub fn approvals_report(",
        "pub fn diff_report(",
        "pub fn evidence_report(",
    ] {
        assert!(
            report_composition.contains(responsibility),
            "TUI report composition owner is missing {responsibility}"
        );
        assert!(
            !tui_composition.contains(responsibility),
            "TUI adapter still owns report composition: {responsibility}"
        );
    }
    assert!(
        tui_composition.lines().count() < 350,
        "TUI adapter regrew beyond its ownership boundary"
    );
    assert!(
        tui_test_source.lines().count() < 550,
        "TUI regression module regrew beyond its ownership boundary"
    );
    assert!(tui_report_test_source.lines().count() < 300);
    assert!(model_switch.lines().count() < 225);
    assert!(interactive_runtime.lines().count() <= 200);
    assert!(
        report_composition.lines().count() < 250,
        "TUI report composition module regrew beyond its ownership boundary"
    );

    let controller = fs::read_to_string("src/surfaces/tui/controller.rs").unwrap();
    for definition in ["trait TuiRuntimePort", "fn run_controller"] {
        assert!(
            controller.contains(definition),
            "TUI controller owner is missing {definition}"
        );
    }
    let terminal_flow = fs::read_to_string("src/surfaces/tui/controller/terminal_flow.rs").unwrap();
    for definition in ["fn terminal_fault_error", "fn consume_outcome"] {
        assert!(
            terminal_flow.contains(definition),
            "TUI terminal-flow owner is missing {definition}"
        );
        assert!(
            !controller.contains(&format!("pub(crate) {definition}")),
            "TUI command loop still defines terminal-flow behavior: {definition}"
        );
    }
    assert!(controller.contains("pub(crate) use terminal_flow::consume_outcome;"));
    assert!(controller.contains("pub(crate) use terminal_flow::terminal_fault_error;"));
    assert!(!controller.contains("use crate::runtime;"));
    assert!(!controller.contains("crate::runtime::"));
    assert!(!controller.contains("crate::adapters"));
    assert!(!terminal_flow.contains("crate::adapters"));
    assert!(interactive_runtime.contains("impl TuiRuntimePort for TuiRuntimeAdapter"));

    let terminal_port = fs::read_to_string("src/runtime_core/terminal.rs").unwrap();
    for definition in [
        "enum TerminalFault",
        "enum FrameWriteBoundary",
        "trait TerminalIo",
    ] {
        assert!(
            terminal_port.contains(definition),
            "terminal contract owner is missing {definition}"
        );
    }
    let native_terminal = fs::read_to_string("src/adapters/terminal/native.rs").unwrap();
    assert!(native_terminal.contains("runtime_core::terminal"));
    assert!(!native_terminal.contains("pub enum TerminalFault"));
    assert!(!native_terminal.contains("pub trait TerminalIo"));

    let render = fs::read_to_string("src/surfaces/tui/render.rs").unwrap();
    assert!(render.contains("fn render_interactive_frame"));
    assert!(!tui_composition.contains("fn render_interactive_frame"));
    let render_notice = fs::read_to_string("src/surfaces/tui/render/notice.rs").unwrap();
    assert!(render_notice.contains("fn render_lines"));
    assert!(!tui_composition.contains("fn render_notice_lines"));
    let render_text = fs::read_to_string("src/surfaces/tui/render/text.rs").unwrap();
    for definition in [
        "fn sanitize_terminal_text",
        "fn truncate_chars",
        "fn display_cell_width",
    ] {
        assert!(
            render_text.contains(definition),
            "TUI terminal-text owner is missing {definition}"
        );
        assert!(
            !tui_composition.contains(definition),
            "TUI adapter still owns {definition}"
        );
    }
    let report_layout = fs::read_to_string("src/surfaces/tui/render/report_layout.rs").unwrap();
    for definition in ["fn terminal_width", "fn push_wrapped", "fn bytes_label"] {
        assert!(
            report_layout.contains(definition),
            "TUI report-layout owner is missing {definition}"
        );
        assert!(
            !tui_composition.contains(definition),
            "TUI adapter still owns {definition}"
        );
    }

    let report_render = fs::read_to_string("src/surfaces/tui/report_render.rs").unwrap();
    assert!(
        report_render.lines().count() < 25,
        "TUI report-render facade exceeded its 25-line budget"
    );
    for owner in [
        "canonical",
        "evidence",
        "monitor",
        "overview",
        "sessions",
        "transcript",
    ] {
        assert!(
            report_render
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "TUI report-render facade does not register {owner}"
        );
    }
    for (owner, definition, line_budget) in [
        ("canonical", "fn canonical_page_report", 125),
        ("evidence", "fn render_evidence_report", 100),
        ("monitor", "fn render_monitor_report", 160),
        ("overview", "fn render_overview_report", 135),
        ("sessions", "fn render_sessions_report", 80),
        ("transcript", "fn render_transcript_report", 130),
    ] {
        let path = format!("src/surfaces/tui/report_render/{owner}.rs");
        let source = fs::read_to_string(&path).unwrap();
        assert!(
            source.contains(definition),
            "TUI report renderer {owner} is missing {definition}"
        );
        assert!(
            !report_render.contains(definition),
            "TUI report-render facade still owns {definition}"
        );
        assert!(
            source.lines().count() < line_budget,
            "TUI report renderer {owner} exceeded its {line_budget}-line budget"
        );
        assert!(
            !tui_composition.contains(definition),
            "TUI adapter still owns {definition}"
        );
    }
    let canonical = fs::read_to_string("src/surfaces/tui/report_render/canonical.rs").unwrap();
    assert!(canonical.contains("fn authority_pair"));
    assert!(!report_render.contains("fn authority_pair"));
}
