use super::backend::vision_status;
use super::TuiRuntimeAdapter;
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiBackendStatus, TuiStatusSnapshot};

pub(super) fn read(adapter: &TuiRuntimeAdapter) -> Result<TuiStatusSnapshot, AppError> {
    let backend = crate::app::inference_adapter::backend::runtime_snapshot()?;
    let identity = crate::app::workflow_adapter::ledger::validated_current_identity()?;
    let latest = (!adapter.fresh_session_pending)
        .then(|| {
            crate::app::observability_adapter::latest_model_run_for_session_read_only(
                &identity.session_id,
            )
            .ok()
            .flatten()
        })
        .flatten();
    let configured_model = crate::app::inference_adapter::model::configured_model_id();
    let model = configured_model
        .clone()
        .or_else(|| backend.model_id.clone())
        .or_else(|| latest.as_ref().map(|run| run.model_id.clone()))
        .unwrap_or_else(|| "미선택".to_string());
    let latest_matches_model = latest.as_ref().is_some_and(|run| run.model_id == model);
    let context_limit_tokens = crate::app::inference_adapter::model::configured_context_length()
        .ok()
        .or(backend.context_limit_tokens)
        .or_else(|| {
            latest
                .as_ref()
                .filter(|_| latest_matches_model)
                .and_then(|run| run.context_limit_tokens)
        });
    let context_tokens_used = latest
        .as_ref()
        .filter(|run| latest_matches_model && run.context_limit_tokens == context_limit_tokens)
        .and_then(|run| run.context_tokens_used);
    let vision = vision_status(Some(&backend));
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
        has_compaction_checkpoint: !adapter.fresh_session_pending
            && crate::app::workflow_adapter::state::current_compaction_boundary(
                &identity.session_id,
            )?
            .is_some(),
        backend,
        vision,
        session_id: if adapter.fresh_session_pending {
            "new".to_string()
        } else {
            identity.session_id
        },
    })
}
