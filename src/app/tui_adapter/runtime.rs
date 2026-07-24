//! Interactive TUI runtime composition.

mod backend;

use super::model_switch::{switch_prepared_model, LiveModelSwitch};
use super::{
    canonical_dispatch_intent, canonical_gate_descriptor, canonical_read_page,
    canonical_selection_lease, conversation, TuiRuntimeAdapter,
};
use crate::foundation::error::AppError;
use crate::surfaces::tui::controller::TuiRuntimePort;
use crate::surfaces::tui::outcome::TuiOutcome;
use crate::surfaces::tui::runtime_bridge::{
    new_tui_intent_id, SelectionLease, TuiAttachment, TuiBackendStatus, TuiConversationTurn,
    TuiGateKind, TuiIntent, TuiReadPage, TuiReadRequest, TuiStatusSnapshot,
};
use backend::ensure_runtime_ready;

struct RequestExecution {
    response: String,
    transcript_owner: TranscriptOwner,
}

enum TranscriptOwner {
    TuiConversation,
    Workflow,
}

impl TuiRuntimePort for TuiRuntimeAdapter {
    fn startup_update_notice(&mut self) -> Option<String> {
        crate::composition::update::startup_notice()
    }

    fn conversation_history(&mut self) -> Result<Vec<TuiConversationTurn>, AppError> {
        super::session_memory::load().map(|memory| memory.turns)
    }

    fn clear_conversation_history(&mut self) -> Result<(), AppError> {
        super::session_memory::clear()
    }

    fn apply_update(&mut self) -> Result<String, AppError> {
        crate::composition::update::update_report()
    }

    fn read_tui_status(&mut self) -> Result<TuiStatusSnapshot, AppError> {
        let backend = crate::app::inference_adapter::backend::runtime_snapshot()?;
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        let latest = crate::app::observability_adapter::latest_model_run_for_session_read_only(
            &identity.session_id,
        )
        .ok()
        .flatten();
        let model = backend
            .model_id
            .clone()
            .or_else(crate::app::inference_adapter::model::configured_model_id)
            .or_else(|| latest.as_ref().map(|run| run.model_id.clone()))
            .unwrap_or_else(|| "미선택".to_string());
        let latest_matches_model = latest.as_ref().is_some_and(|run| run.model_id == model);
        let context_tokens_used = latest
            .as_ref()
            .filter(|_| latest_matches_model)
            .and_then(|run| run.context_tokens_used);
        let context_limit_tokens = backend.context_limit_tokens.or_else(|| {
            latest
                .as_ref()
                .filter(|_| latest_matches_model)
                .and_then(|run| run.context_limit_tokens)
        });
        let vision_ready = backend.vision_ready;
        let backend = match backend.status {
            "ready" => TuiBackendStatus::Ready,
            "stale" => TuiBackendStatus::Stale,
            "stopped" => TuiBackendStatus::Stopped,
            _ => TuiBackendStatus::Unavailable,
        };
        Ok(TuiStatusSnapshot {
            model,
            context_tokens_used,
            context_limit_tokens,
            has_compaction_checkpoint:
                crate::app::workflow_adapter::state::current_compaction_boundary(
                    &identity.session_id,
                )?
                .is_some(),
            backend,
            vision_ready,
            session_id: identity.session_id,
        })
    }

    fn compact_context(&mut self) -> Result<String, AppError> {
        Ok(crate::app::context_adapter::compact_manually()?.report())
    }

    fn capture_attachment(&mut self, path: &str) -> Result<TuiAttachment, AppError> {
        let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
        super::attachment::capture(path, &identity.session_id)
    }

    fn submit_request(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
    ) -> Result<String, AppError> {
        let memory = super::session_memory::load()?;
        let execution = self.execute_request(request, attachments, &memory.turns)?;
        if matches!(execution.transcript_owner, TranscriptOwner::TuiConversation) {
            super::session_memory::record_exchange(&memory, request.trim(), &execution.response)?;
        }
        Ok(execution.response)
    }

    fn model_options(&mut self) -> Vec<crate::surfaces::tui::runtime_bridge::TuiModelOption> {
        crate::app::inference_adapter::model::setup_options()
    }

    fn setup_model(&mut self, id: &str) -> Result<String, AppError> {
        crate::app::inference_adapter::backend::ensure_installed_report()?;
        let prepared = crate::app::inference_adapter::model::prepare_setup_model(id)?;
        let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
        let default = crate::app::inference_adapter::model::snapshot_default_selection()?;
        switch_prepared_model(
            &mut LiveModelSwitch,
            &prepared.id,
            &prepared.artifact_path.display().to_string(),
            prepared.context_tokens,
            &snapshot,
            &default,
        )?;
        Ok(format!(
            "모델 변경 완료\n- model: {}\n- context: {}\n- vision: {}\n- backend: ready",
            prepared.id,
            prepared.context_tokens,
            if prepared.vision_ready {
                "ready"
            } else {
                "text-only"
            }
        ))
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

impl TuiRuntimeAdapter {
    fn execute_request(
        &mut self,
        request: &str,
        attachments: &[TuiAttachment],
        history: &[TuiConversationTurn],
    ) -> Result<RequestExecution, AppError> {
        let user_request = request.trim();
        let backend = crate::app::inference_adapter::backend::runtime_snapshot().ok();
        let context_limit_tokens = backend
            .as_ref()
            .and_then(|snapshot| snapshot.context_limit_tokens)
            .or_else(|| crate::app::inference_adapter::model::configured_context_length().ok());
        let active_model = backend
            .and_then(|snapshot| snapshot.model_id)
            .or_else(crate::app::inference_adapter::model::configured_model_id);
        let input = super::attachment::compose_request(request, attachments, context_limit_tokens)?;
        let local_context = input.text.as_str();
        if !input.images.is_empty() {
            ensure_runtime_ready()?;
            return conversation::reply_with_images(
                &input,
                history,
                required_context_limit(context_limit_tokens)?,
            )
            .map(tui_execution);
        }
        if let Some(result) =
            super::web_tools::dispatch(&mut self.opened_web_page, user_request, local_context)
        {
            return result.map(tui_execution);
        }
        if let Some(reply) = conversation::local_reply(user_request, active_model.as_deref()) {
            return Ok(tui_execution(reply));
        }
        ensure_runtime_ready()?;
        let conversational = conversation::is_conversational_request(user_request);
        let has_text_attachments = !attachments.is_empty();
        match conversation::decide_request(
            user_request,
            history,
            required_context_limit(context_limit_tokens)?,
            conversational && !has_text_attachments,
        )? {
            conversation::RequestDecision::Answer(answer) => return Ok(tui_execution(answer)),
            conversation::RequestDecision::WebTool(tool) => {
                return super::web_tools::execute(
                    &mut self.opened_web_page,
                    tool,
                    user_request,
                    local_context,
                )
                .map(tui_execution);
            }
            conversation::RequestDecision::ContinueLocal => {}
        }
        if conversational {
            return conversation::reply_with_context(
                user_request,
                local_context,
                history,
                required_context_limit(context_limit_tokens)?,
            )
            .map(tui_execution);
        }
        crate::app::runtime_adapter::agent_run_report(local_context).map(|report| {
            RequestExecution {
                response: conversation::present_agent_report(&report),
                transcript_owner: TranscriptOwner::Workflow,
            }
        })
    }
}

fn tui_execution(response: String) -> RequestExecution {
    RequestExecution {
        response,
        transcript_owner: TranscriptOwner::TuiConversation,
    }
}

fn required_context_limit(context_limit_tokens: Option<u32>) -> Result<u32, AppError> {
    context_limit_tokens.filter(|value| *value > 0).ok_or_else(|| {
        AppError::blocked(
            "선택한 모델의 context length를 확인하지 못했습니다. /model에서 모델을 다시 선택하거나 /doctor로 backend 상태를 확인하세요.",
        )
    })
}
