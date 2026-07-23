//! Interactive TUI runtime composition.

use crate::foundation::error::AppError;
use crate::surfaces::tui::controller::TuiRuntimePort;
use crate::surfaces::tui::outcome::TuiOutcome;
use crate::surfaces::tui::runtime_bridge::{
    new_tui_intent_id, SelectionLease, TuiAttachment, TuiBackendStatus, TuiGateKind, TuiIntent,
    TuiReadPage, TuiReadRequest, TuiStatusSnapshot,
};
use crate::surfaces::tui::setup;

use super::model_switch::{switch_prepared_model, LiveModelSwitch};
use super::{
    canonical_dispatch_intent, canonical_gate_descriptor, canonical_read_page,
    canonical_selection_lease, conversation, TuiRuntimeAdapter,
};

impl TuiRuntimePort for TuiRuntimeAdapter {
    fn startup_update_notice(&mut self) -> Option<String> {
        crate::composition::update::startup_notice()
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
        let request = super::attachment::compose_request(request, attachments)?;
        let active_model = crate::app::inference_adapter::backend::runtime_snapshot()
            .ok()
            .and_then(|snapshot| snapshot.model_id)
            .or_else(crate::app::inference_adapter::model::configured_model_id);
        if let Some(reply) = conversation::local_reply(&request, active_model.as_deref()) {
            return Ok(reply);
        }
        ensure_runtime_ready()?;
        if crate::app::web_search_adapter::should_search(&request) {
            return crate::app::web_search_adapter::answer(&request);
        }
        if conversation::is_conversational_request(&request) {
            return conversation::reply(&request);
        }
        crate::app::runtime_adapter::agent_run_report(&request)
            .map(|report| conversation::present_agent_report(&report))
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
            &snapshot,
            &default,
        )?;
        Ok(format!(
            "모델 변경 완료\n- model: {}\n- context: {}\n- backend: ready",
            prepared.id,
            setup::DEFAULT_CONTEXT_TOKENS
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

fn ensure_runtime_ready() -> Result<(), AppError> {
    let snapshot = crate::app::inference_adapter::backend::runtime_snapshot()?;
    if snapshot.status == "ready" {
        return Ok(());
    }
    if snapshot.status == "stale" {
        crate::app::inference_adapter::backend::stop_report()?;
    }
    let model_path =
        crate::app::inference_adapter::model::default_artifact_path().map_err(|err| {
            if err.message.contains("기본 모델이 선택되지 않았습니다") {
                AppError::blocked(
                    "모델이 선택되지 않았습니다. TUI에서 /model을 입력해 모델을 선택하세요.",
                )
            } else {
                err
            }
        })?;
    crate::app::inference_adapter::backend::ensure_installed_report()?;
    crate::app::inference_adapter::backend::start_report(
        &model_path.display().to_string(),
        Some(setup::DEFAULT_CONTEXT_TOKENS),
    )?;
    Ok(())
}
