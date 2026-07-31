use super::*;
use crate::adapters::terminal::native::ScriptedTerminal;
use crate::foundation::error::AppError;
use crate::surfaces::tui::controller::{run_controller, TuiRuntimePort};
use crate::surfaces::tui::render::{
    display_cell_width, render_interactive_frame, render_interactive_frame_with_options,
};
use crate::surfaces::tui::runtime_bridge::{
    SelectionLease, TuiAttachment, TuiAttachmentKind, TuiConversationRole, TuiConversationTurn,
    TuiFreshness, TuiGateKind, TuiIntent, TuiModelOption, TuiReadContinuation, TuiReadPage,
    TuiReadRequest, TuiSessionOption, TuiSessionTransition, TuiStatusSnapshot, TuiWebSourceOption,
};
use crate::surfaces::tui::view_model::{ConversationRole, InteractiveState};

#[derive(Default)]
struct ConversationRuntime {
    history: Vec<TuiConversationTurn>,
    reconcile_backend_calls: usize,
    clear_history_calls: usize,
    requests: Vec<String>,
    page_reads: usize,
    update_calls: usize,
    model_options: Vec<TuiModelOption>,
    setup_models: Vec<String>,
    captured_paths: Vec<String>,
    submitted_attachment_counts: Vec<usize>,
    submit_failures_remaining: usize,
    session_options: Vec<TuiSessionOption>,
    resumed_sessions: Vec<String>,
    new_session_calls: usize,
    web_source_options: Vec<TuiWebSourceOption>,
    selected_web_sources: Vec<String>,
    progress_hint: Option<String>,
    submit_delay_ms: u64,
    context_estimate: Option<u32>,
    status_failure: Option<String>,
    cooperatively_stalled: bool,
}

impl TuiRuntimePort for ConversationRuntime {
    fn startup_update_notice(&mut self) -> Option<String> {
        None
    }

    fn reconcile_existing_backend(&mut self) -> Result<(), AppError> {
        self.reconcile_backend_calls += 1;
        Ok(())
    }

    fn clear_conversation_history(&mut self) -> Result<(), AppError> {
        self.clear_history_calls += 1;
        self.history.clear();
        Ok(())
    }

    fn apply_update(&mut self) -> Result<String, AppError> {
        self.update_calls += 1;
        Ok("업데이트 완료".to_string())
    }

    fn read_tui_page(&mut self, _request: TuiReadRequest) -> Result<TuiReadPage, AppError> {
        self.page_reads += 1;
        Ok(TuiReadPage {
            title: "overview".to_string(),
            lines: vec!["ledger: must stay hidden".to_string()],
            page: 0,
            has_previous: false,
            has_next: false,
            freshness: TuiFreshness::Fresh,
            continuation: TuiReadContinuation::Complete,
            authority: crate::surfaces::tui::runtime_bridge::TuiReadAuthority::default(),
        })
    }

    fn read_tui_status(&mut self) -> Result<TuiStatusSnapshot, AppError> {
        if let Some(message) = &self.status_failure {
            return Err(AppError::runtime(message));
        }
        Ok(TuiStatusSnapshot::unavailable())
    }

    fn model_options(&mut self) -> Vec<TuiModelOption> {
        self.model_options.clone()
    }

    fn session_options(&mut self) -> Result<Vec<TuiSessionOption>, AppError> {
        Ok(self.session_options.clone())
    }

    fn web_source_options(&mut self) -> Vec<TuiWebSourceOption> {
        self.web_source_options.clone()
    }

    fn select_web_source(&mut self, source_id: &str) -> Result<String, AppError> {
        self.selected_web_sources.push(source_id.to_string());
        Ok(format!("현재 웹 출처를 변경했습니다: {source_id}"))
    }

    fn start_new_session(&mut self) -> Result<TuiSessionTransition, AppError> {
        self.new_session_calls += 1;
        self.history.clear();
        Ok(TuiSessionTransition {
            session_id: "session-new".to_string(),
            notice: "새 세션을 시작했습니다.".to_string(),
            turns: Vec::new(),
        })
    }

    fn resume_session(&mut self, session_id: &str) -> Result<TuiSessionTransition, AppError> {
        self.resumed_sessions.push(session_id.to_string());
        Ok(TuiSessionTransition {
            session_id: session_id.to_string(),
            notice: "세션을 재개했습니다.".to_string(),
            turns: self.history.clone(),
        })
    }

    fn setup_model(&mut self, id: &str) -> Result<String, AppError> {
        self.setup_models.push(id.to_string());
        Ok(format!("모델 적용 완료: {id}"))
    }

    fn doctor_report(&mut self) -> String {
        String::new()
    }

    fn compact_context(&mut self) -> Result<String, AppError> {
        unreachable!()
    }

    fn capture_attachment(&mut self, path: &str) -> Result<TuiAttachment, AppError> {
        self.captured_paths.push(path.to_string());
        Ok(TuiAttachment {
            id: "attachment-test".to_string(),
            display_name: path.to_string(),
            stored_path: path.to_string(),
            size_bytes: 1,
            kind: TuiAttachmentKind::Image,
        })
    }

    fn request_progress_hint(&mut self, _request: &str) -> Option<String> {
        self.progress_hint.clone()
    }

    fn request_context_tokens_hint(
        &mut self,
        _request: &str,
        _attachments: &[TuiAttachment],
    ) -> Option<u32> {
        self.context_estimate
    }

    fn submit_request(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
    ) -> Result<String, AppError> {
        if self.submit_delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.submit_delay_ms));
        }
        self.requests.push(request.to_string());
        self.submitted_attachment_counts.push(attachments.len());
        if self.submit_failures_remaining > 0 {
            self.submit_failures_remaining -= 1;
            return Err(AppError::runtime("테스트 요청 실패"));
        }
        Ok("안녕하세요.".to_string())
    }

    fn submit_request_with_progress(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
        _progress: &crate::surfaces::tui::runtime_bridge::TuiRequestProgressReporter,
        cancellation: &crate::runtime_core::inference::cancellation::RequestCancellationToken,
    ) -> Result<String, AppError> {
        if !self.cooperatively_stalled {
            return self.submit_request(request, attachments);
        }
        self.requests.push(request.to_string());
        while !cancellation.is_cancelled() {
            std::thread::yield_now();
        }
        cancellation.check()?;
        unreachable!()
    }

    fn new_tui_intent_id(&mut self) -> String {
        "intent-test".to_string()
    }

    fn tui_selection_lease(
        &mut self,
        _selected_object_id: &str,
    ) -> Result<SelectionLease, AppError> {
        unreachable!()
    }

    fn tui_gate_descriptor(
        &mut self,
        _workflow_id: &str,
    ) -> Result<(String, TuiGateKind), AppError> {
        unreachable!()
    }

    fn dispatch_tui_intent(&mut self, _intent: TuiIntent) -> Result<TuiOutcome, AppError> {
        unreachable!()
    }
}

fn model_option(id: &str, display_name: &str, current: bool, recommended: bool) -> TuiModelOption {
    TuiModelOption {
        id: id.to_string(),
        display_name: display_name.to_string(),
        quantization: "Q4".to_string(),
        download_bytes: 1024,
        model_cached: false,
        vision_projector_bytes: Some(512),
        vision_projector_cached: false,
        context_length: Some(4096),
        ram: "4 GiB".to_string(),
        license: "Apache-2.0".to_string(),
        note: "test model".to_string(),
        current,
        evaluation_recommended: recommended,
        readiness: crate::surfaces::tui::runtime_bridge::TuiModelReadiness::EvaluationOnly,
    }
}

fn session_option(session_id: &str, preview: &str, current: bool) -> TuiSessionOption {
    TuiSessionOption {
        session_id: session_id.to_string(),
        preview: preview.to_string(),
        current,
    }
}

fn web_source_option(source_id: &str, title: &str, url: &str, current: bool) -> TuiWebSourceOption {
    TuiWebSourceOption {
        source_id: source_id.to_string(),
        title: title.to_string(),
        url: url.to_string(),
        opened: current,
        current,
    }
}

fn strip_ansi(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{001b}' {
            output.push(ch);
            continue;
        }
        if chars.next_if_eq(&'[').is_none() {
            continue;
        }
        for next in chars.by_ref() {
            if ('@'..='~').contains(&next) {
                break;
            }
        }
    }
    output
}

#[path = "conversation_tests/attachment_layout.rs"]
mod attachment_layout;
#[path = "conversation_tests/progress_model.rs"]
mod progress_model;
#[path = "conversation_tests/rendering.rs"]
mod rendering;
#[path = "conversation_tests/session.rs"]
mod session;
#[path = "conversation_tests/web.rs"]
mod web;
