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
use crate::surfaces::tui::outcome::TuiOutcome;
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
        self.conversation_memory()?;
        let mut memory = self
            .conversation_memory
            .take()
            .ok_or_else(|| AppError::blocked("conversation memory 초기화 실패"))?;
        let execution = request::execute(
            self,
            request,
            attachments,
            memory.turns(),
            memory.web_grounding(),
        );
        let result = match execution {
            Ok(execution) => {
                let recorded = super::session_memory::record_exchange(
                    &mut memory,
                    request.trim(),
                    &execution.response,
                    &execution.web_grounding,
                );
                recorded.map(|()| execution.response)
            }
            Err(error) => {
                let recorded = super::session_memory::record_failure(
                    &mut memory,
                    request.trim(),
                    &error.message,
                );
                match recorded {
                    Ok(()) => Err(error),
                    Err(record_error) => Err(record_error),
                }
            }
        };
        self.conversation_memory = Some(memory);
        result
    }

    fn model_options(&mut self) -> Vec<crate::surfaces::tui::runtime_bridge::TuiModelOption> {
        crate::app::inference_adapter::model::setup_options()
    }

    fn session_options(&mut self) -> Result<Vec<TuiSessionOption>, AppError> {
        self.available_sessions()
    }

    fn web_source_options(&mut self) -> Vec<TuiWebSourceOption> {
        web_sources::options(&self.web_pages)
    }

    fn select_web_source(&mut self, source_id: &str) -> Result<String, AppError> {
        web_sources::select(&mut self.web_pages, source_id)
    }

    fn start_new_session(&mut self) -> Result<TuiSessionTransition, AppError> {
        self.start_session()
    }

    fn resume_session(&mut self, session_id: &str) -> Result<TuiSessionTransition, AppError> {
        self.resume_selected_session(session_id)
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
