//! Interactive TUI runtime composition.

mod backend;
mod model_setup;
mod request;
#[cfg(test)]
mod session_tests;
mod state;
mod status;
mod web_sources;

use super::{
    canonical_dispatch_intent, canonical_gate_descriptor, canonical_read_page,
    canonical_selection_lease,
};
use crate::foundation::error::AppError;
use crate::surfaces::tui::controller::TuiRuntimePort;
use crate::surfaces::tui::outcome::{TuiEffect, TuiOutcome};
use crate::surfaces::tui::runtime_bridge::{
    new_tui_intent_id, SelectionLease, TuiAttachment, TuiGateKind, TuiIntent, TuiReadPage,
    TuiReadRequest, TuiSessionOption, TuiSessionTransition, TuiStatusSnapshot, TuiWebSourceOption,
};
use backend::reconcile_existing_runtime;
pub(super) use state::TuiRuntimeAdapter;

impl TuiRuntimePort for TuiRuntimeAdapter {
    fn startup_update_notice(&mut self) -> Option<String> {
        crate::composition::update::startup_notice()
    }

    fn reconcile_existing_backend(&mut self) -> Result<(), AppError> {
        reconcile_existing_runtime()
    }

    fn clear_conversation_history(&mut self) -> Result<(), AppError> {
        if self.fresh_session_pending {
            return Ok(());
        }
        super::session_memory::clear(self.conversation_memory()?)
    }

    fn apply_update(&mut self) -> Result<String, AppError> {
        crate::composition::update::update_report()
    }

    fn read_tui_status(&mut self) -> Result<TuiStatusSnapshot, AppError> {
        status::read(self)
    }

    fn compact_context(&mut self) -> Result<String, AppError> {
        self.ensure_fresh_session()?;
        Ok(crate::app::context_adapter::compact_manually()?.report())
    }

    fn capture_attachment(&mut self, path: &str) -> Result<TuiAttachment, AppError> {
        self.ensure_fresh_session()?;
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        super::attachment::capture(path, &identity.session_id)
    }

    fn request_progress_hint(&mut self, request: &str) -> Option<String> {
        crate::app::browser_adapter::progress_notice(request)
    }

    fn request_context_tokens_hint(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
    ) -> Option<u32> {
        status::estimate_context_tokens(self, request, attachments)
    }

    fn submit_request(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
    ) -> Result<String, AppError> {
        self.ensure_fresh_session()?;
        let history = self.conversation_memory()?.prompt_history();
        match request::execute(self, request, attachments, &history) {
            Ok(response) => {
                super::session_memory::record_exchange(
                    self.conversation_memory()?,
                    request.trim(),
                    &response,
                )?;
                Ok(response)
            }
            Err(error) => {
                super::session_memory::record_failure(
                    self.conversation_memory()?,
                    request.trim(),
                    &error.message,
                )?;
                Err(error)
            }
        }
    }

    fn model_options(&mut self) -> Vec<crate::surfaces::tui::runtime_bridge::TuiModelOption> {
        crate::app::inference_adapter::model::setup_options()
    }

    fn session_options(&mut self) -> Result<Vec<TuiSessionOption>, AppError> {
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        Ok(crate::app::observability_adapter::session_history(20)?
            .into_iter()
            .map(|session| TuiSessionOption {
                current: !self.fresh_session_pending && session.session_id == identity.session_id,
                preview: session
                    .last_summary
                    .unwrap_or_else(|| "저장된 대화".to_string()),
                session_id: session.session_id,
            })
            .collect())
    }

    fn web_source_options(&mut self) -> Vec<TuiWebSourceOption> {
        web_sources::options(&self.web_pages)
    }

    fn select_web_source(&mut self, source_id: &str) -> Result<String, AppError> {
        web_sources::select(&mut self.web_pages, source_id)
    }

    fn start_new_session(&mut self) -> Result<TuiSessionTransition, AppError> {
        crate::app::workflow_adapter::state::session_new_report()?;
        self.conversation_memory = None;
        self.web_pages.clear();
        self.fresh_session_pending = false;
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        Ok(TuiSessionTransition {
            session_id: identity.session_id,
            notice: "새 세션을 시작했습니다.".to_string(),
            turns: self.conversation_memory()?.turns.clone(),
        })
    }

    fn resume_session(&mut self, session_id: &str) -> Result<TuiSessionTransition, AppError> {
        let intent_id = new_tui_intent_id();
        let lease = canonical_selection_lease(session_id)?;
        let outcome = canonical_dispatch_intent(TuiIntent::ResumeSession {
            intent_id,
            session_id: session_id.to_string(),
            lease,
        })?;
        if outcome.effect != TuiEffect::Committed {
            return Err(AppError::blocked(outcome.safe_message));
        }
        self.conversation_memory = None;
        self.web_pages.clear();
        self.fresh_session_pending = false;
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        Ok(TuiSessionTransition {
            session_id: identity.session_id,
            notice: "선택한 세션을 재개했습니다.".to_string(),
            turns: self.conversation_memory()?.turns.clone(),
        })
    }

    fn setup_model(&mut self, id: &str) -> Result<String, AppError> {
        model_setup::setup(id)
    }

    fn doctor_report(&mut self) -> String {
        crate::app::runtime_adapter::doctor_report()
    }

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
        canonical_selection_lease(selected_object_id)
    }

    fn tui_gate_descriptor(
        &mut self,
        workflow_id: &str,
    ) -> Result<(String, TuiGateKind), AppError> {
        canonical_gate_descriptor(workflow_id)
    }

    fn dispatch_tui_intent(&mut self, intent: TuiIntent) -> Result<TuiOutcome, AppError> {
        canonical_dispatch_intent(intent)
    }
}
