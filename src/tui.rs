use crate::adapters::terminal::capability;
use crate::adapters::terminal::native::NativeTerminal;
use crate::composition::tui_read::{self, TuiReadPort};
use crate::foundation::error::AppError;
use crate::runtime;
pub(crate) use crate::surfaces::tui::controller::terminal_fault_error;
use crate::surfaces::tui::controller::{self, TuiRuntimePort};
use crate::surfaces::tui::outcome::TuiOutcome;
use crate::surfaces::tui::page::ProjectionStatus;
use crate::surfaces::tui::runtime_bridge::{
    new_tui_intent_id, SelectionLease, TuiGateKind, TuiIntent, TuiReadBudget, TuiReadPage,
    TuiReadRequest,
};

pub fn run_auto() -> Result<(), AppError> {
    if capability::attached() {
        let mut terminal = NativeTerminal::new();
        controller::run_controller(&mut terminal, &mut LegacyTuiRuntimePort)
    } else {
        println!("{}", overview_report()?);
        Ok(())
    }
}

pub fn run_interactive() -> Result<(), AppError> {
    let mut terminal = NativeTerminal::explicit_line_mode();
    controller::run_controller(&mut terminal, &mut LegacyTuiRuntimePort)
}

struct LegacyTuiRuntimePort;

pub(crate) struct LegacyTuiReadPort;

impl TuiReadPort for LegacyTuiReadPort {
    fn state_snapshot(
        &mut self,
        max_ledger_events: usize,
    ) -> Result<crate::runtime_core::workflow::domain::snapshot::TuiStateSnapshot, AppError> {
        crate::state::tui_state_snapshot_read_only(max_ledger_events)
    }

    fn store_status(
        &mut self,
    ) -> Result<crate::runtime_core::observability::facade::StoreStatus, AppError> {
        crate::observability::status_read_only()
    }

    fn monitor_snapshot(
        &mut self,
        limit: usize,
    ) -> Result<crate::runtime_core::observability::facade::MonitorProjectionSnapshot, AppError>
    {
        crate::observability::monitor_snapshot_read_only(limit)
    }

    fn transcript_record(
        &mut self,
        event: &crate::runtime_core::workflow::storage_compat::ledger::ParsedLedgerEvent,
    ) -> Result<crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord, AppError>
    {
        crate::transcript::record_from_event(event)
    }

    fn tool_output_view(
        &mut self,
        record: &crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord,
        artifact_id: &str,
    ) -> Result<crate::runtime_core::workflow::domain::transcript::ToolOutputView, AppError> {
        crate::transcript::tool_output_view_from_canonical_record(record, artifact_id)
    }

    fn proposal_detail(
        &mut self,
        workflow: &crate::runtime_core::workflow::storage_compat::record::WorkflowRecord,
        proposal_id: &str,
        max_bytes: usize,
    ) -> Result<crate::runtime_core::patch::proposal::PatchProposalDetail, AppError> {
        crate::patch::proposal_detail_for_workflow_bounded(workflow, proposal_id, max_bytes)
    }

    fn evidence_status(
        &mut self,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<crate::runtime_core::knowledge::evidence::EvidenceStoreStatus, AppError> {
        crate::evidence::store_status_bounded(max_entries, max_bytes)
    }

    fn content_hash(&mut self, value: &str) -> String {
        crate::state::sha256_text(value)
    }

    fn projection_status(&mut self, project_id: &str) -> ProjectionStatus {
        match crate::transition::projection_lag_status_read_only(project_id) {
            crate::transition::ProjectionLagReadStatus::Clear => ProjectionStatus::Clear,
            crate::transition::ProjectionLagReadStatus::Lagging => ProjectionStatus::Lagging,
            crate::transition::ProjectionLagReadStatus::Unavailable => {
                ProjectionStatus::Unavailable
            }
        }
    }
}

pub(crate) fn canonical_read_page(request: TuiReadRequest) -> Result<TuiReadPage, AppError> {
    tui_read::read_tui_page(&mut LegacyTuiReadPort, request)
}

impl TuiRuntimePort for LegacyTuiRuntimePort {
    fn read_tui_page(&mut self, request: TuiReadRequest) -> Result<TuiReadPage, AppError> {
        canonical_read_page(request)
    }

    fn new_tui_intent_id(&mut self) -> String {
        new_tui_intent_id()
    }

    fn tui_selection_lease(
        &mut self,
        selected_object_id: &str,
    ) -> Result<SelectionLease, AppError> {
        runtime::tui_selection_lease(selected_object_id)
    }

    fn tui_gate_descriptor(
        &mut self,
        workflow_id: &str,
    ) -> Result<(String, TuiGateKind), AppError> {
        runtime::tui_gate_descriptor(workflow_id)
    }

    fn dispatch_tui_intent(&mut self, intent: TuiIntent) -> Result<TuiOutcome, AppError> {
        runtime::dispatch_tui_intent(intent)
    }
}

mod report_composition {
    use super::{canonical_read_page, AppError, TuiReadBudget, TuiReadRequest};
    use crate::adapters::filesystem::layout as paths;
    use crate::surfaces::tui::render::terminal_width;
    use crate::surfaces::tui::report_render::{
        canonical_page_report, render_evidence_report, render_monitor_report,
        render_overview_report, render_sessions_report, render_transcript_report,
    };
    use crate::surfaces::tui::view_model::{
        EvidenceReportView, ModelMetricView, MonitorReportView, MonitorStoreView,
        OverviewReportView, OverviewStoreView, ResourceSampleView, SessionSummaryView,
        SessionsReportView, TimelineEventView, TranscriptRecordView, TranscriptReportView,
        TranscriptSessionView,
    };
    use crate::{evidence, ledger, model, observability};

    pub fn overview_report() -> Result<String, AppError> {
        let width = terminal_width();
        let store = observability::status()?;
        let models = observability::model_summaries()?;
        let sessions = observability::session_history(5)?;
        let identity = ledger::validated_current_identity()?;
        Ok(render_overview_report(
            width,
            &OverviewReportView {
                project_root: identity.project_root,
                session_id: identity.session_id,
                store: OverviewStoreView {
                    path: store.path.display().to_string(),
                    recovered_from: store.recovered_from.map(|path| path.display().to_string()),
                    ledger_events: store.ledger_events,
                    sessions: store.sessions,
                    workflows: store.workflows,
                    transcript_records: store.transcript_records,
                },
                models: models.into_iter().map(model_metric_view).collect(),
                candidate_summary: model::candidate_summary(),
                recent_sessions: sessions.into_iter().map(session_summary_view).collect(),
            },
        ))
    }

    pub fn monitor_report() -> Result<String, AppError> {
        let width = terminal_width();
        let store = observability::status()?;
        let models = observability::model_summaries()?;
        let resource = observability::latest_resource_sample()?;
        Ok(render_monitor_report(
            width,
            &MonitorReportView {
                store: MonitorStoreView {
                    path: store.path.display().to_string(),
                    migration_version: store.migration_version,
                    model_runs: store.model_runs,
                    token_records: store.token_records,
                    transcript_records: store.transcript_records,
                    resource_samples: store.resource_samples,
                },
                models: models.into_iter().map(model_metric_view).collect(),
                resource: resource.map(|sample| ResourceSampleView {
                    resource_sample_id: sample.resource_sample_id,
                    backend_id: sample.backend_id,
                    pid: sample.pid,
                    process_cpu_percent: sample.process_cpu_percent,
                    average_rss_bytes: sample.average_rss_bytes,
                    peak_rss_bytes: sample.peak_rss_bytes,
                    disk_bytes: sample.disk_bytes,
                    sample_count: sample.sample_count,
                    pressure_status: sample.pressure_status,
                    recorded_at_ms: sample.recorded_at_ms,
                }),
                candidate_summary: model::candidate_summary(),
            },
        ))
    }

    pub fn sessions_report() -> Result<String, AppError> {
        let width = terminal_width();
        let identity = ledger::validated_current_identity()?;
        let sessions = observability::session_history(10)?;
        Ok(render_sessions_report(
            width,
            &SessionsReportView {
                project_root: identity.project_root,
                current_session_id: identity.session_id,
                state_path: paths::current_state_file().display().to_string(),
                sessions: sessions
                    .into_iter()
                    .map(|session| SessionSummaryView {
                        session_id: session.session_id,
                        event_count: session.event_count,
                        last_summary: session.last_summary,
                    })
                    .collect(),
            },
        ))
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
        let transcript = crate::transcript::records_for_session(session_id)?;

        Ok(render_transcript_report(
            width,
            &TranscriptReportView {
                session: TranscriptSessionView {
                    project_root: session.project_root,
                    session_id: session.session_id,
                    started_at_ms: session.started_at_ms,
                    last_event_at_ms: session.last_event_at_ms,
                    event_count: session.event_count,
                },
                records: transcript
                    .into_iter()
                    .map(|record| TranscriptRecordView {
                        kind: record.kind,
                        workflow_id: record.workflow_id,
                        content: record.content,
                    })
                    .collect(),
                events: events
                    .into_iter()
                    .map(|event| TimelineEventView {
                        event_id: event.event_id,
                        ts_ms: event.ts_ms,
                        event_type: event.event_type,
                        summary: event.summary,
                    })
                    .collect(),
            },
        ))
    }

    pub fn approvals_report() -> Result<String, AppError> {
        let page = canonical_read_page(TuiReadRequest::Approvals {
            page: 0,
            budget: TuiReadBudget::bounded(40, 64 * 1024),
        })?;
        Ok(canonical_page_report(page))
    }

    pub fn diff_report(proposal_id: &str) -> Result<String, AppError> {
        let page = canonical_read_page(TuiReadRequest::Diff {
            proposal_id: proposal_id.to_string(),
            page: 0,
            budget: TuiReadBudget::bounded(120, 64 * 1024),
        })?;
        Ok(canonical_page_report(page))
    }

    fn model_metric_view(
        summary: crate::runtime_core::observability::facade::ModelMetricSummary,
    ) -> ModelMetricView {
        ModelMetricView {
            model_id: summary.model_id,
            runs: summary.runs,
            prompt_tokens: summary.prompt_tokens,
            completion_tokens: summary.completion_tokens,
            total_tokens: summary.total_tokens,
            avg_latency_ms: summary.avg_latency_ms,
            avg_tokens_per_second: summary.avg_tokens_per_second,
        }
    }

    fn session_summary_view(session: observability::SessionHistoryEntry) -> SessionSummaryView {
        SessionSummaryView {
            session_id: session.session_id,
            event_count: session.event_count,
            last_summary: session.last_summary,
        }
    }

    pub fn evidence_report() -> Result<String, AppError> {
        let width = terminal_width();
        let identity = ledger::validated_current_identity()?;
        let store = observability::status()?;
        let evidence = evidence::store_status()?;
        Ok(render_evidence_report(
            width,
            &EvidenceReportView {
                project_root: identity.project_root,
                session_id: identity.session_id,
                runtime_evidence_file: evidence.runtime_evidence_file.display().to_string(),
                runtime_evidence_records: evidence.runtime_evidence_records,
                project_evidence_dir: evidence.project_evidence_dir.display().to_string(),
                project_artifacts: evidence.project_artifacts,
                observability_path: store.path.display().to_string(),
                evidence_records: store.evidence_records,
                stop_gate_results: store.stop_gate_results,
                stale_policy: evidence.stale_policy.to_string(),
            },
        ))
    }
}

pub use report_composition::{
    approvals_report, diff_report, evidence_report, monitor_report, overview_report,
    sessions_report, transcript_report,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::filesystem::layout as paths;
    use crate::adapters::terminal::native::{ScriptedTerminal, TerminalFault};
    use crate::surfaces::tui::controller::{consume_outcome, run_controller};
    use crate::surfaces::tui::outcome::verification_credential_issued;
    use crate::surfaces::tui::render::{render_interactive_frame, sanitize_terminal_text};
    use crate::surfaces::tui::runtime_bridge::{OneShotSecret, TuiFreshness, TuiReadContinuation};
    use crate::surfaces::tui::view_model::{InteractiveState, InteractiveView};
    use crate::{ledger, observability, patch};

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
        let live = include_str!("surfaces/tui/controller.rs");
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
        let source = include_str!("tui.rs");
        let composition = source
            .split_once("mod report_composition {")
            .unwrap()
            .1
            .split_once("\npub use report_composition")
            .unwrap()
            .0;

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
        crate::state::initialize().unwrap();
        let mut terminal = ScriptedTerminal::new(["help", "quit"]);

        run_controller(&mut terminal, &mut LegacyTuiRuntimePort).unwrap();

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
            notice: "결과 제목\n- code: exact.test\n- 동작: 상태를 변경하지 않았습니다."
                .to_string(),
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
        crate::state::initialize().unwrap();

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
        crate::state::initialize().unwrap();

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
        crate::state::initialize().unwrap();

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
}
