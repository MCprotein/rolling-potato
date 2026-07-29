use super::backend::vision_status;
use super::TuiRuntimeAdapter;
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{TuiAttachment, TuiBackendStatus, TuiStatusSnapshot};

pub(super) fn estimate_context_tokens(
    adapter: &mut TuiRuntimeAdapter,
    request: &str,
    attachments: &[TuiAttachment],
) -> Option<u32> {
    let limit = crate::app::inference_adapter::model::configured_context_length().ok()?;
    let input =
        super::super::attachment::compose_request(request, attachments, Some(limit)).ok()?;
    let history = if adapter.fresh_session_pending {
        Vec::new()
    } else {
        adapter.conversation_memory().ok()?.turns().to_vec()
    };
    super::super::conversation::estimate_context_tokens(request, &input, &history, limit).ok()
}

pub(super) fn read(adapter: &mut TuiRuntimeAdapter) -> Result<TuiStatusSnapshot, AppError> {
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
    let configured_runtime_model = crate::app::inference_adapter::model::configured_runtime_spec()
        .ok()
        .and_then(|runtime| {
            runtime
                .model_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        });
    let model = configured_model
        .clone()
        .or_else(|| backend.model_id.clone())
        .or_else(|| latest.as_ref().map(|run| run.model_id.clone()))
        .unwrap_or_else(|| "미선택".to_string());
    let latest_matches_model = latest.as_ref().is_some_and(|run| {
        same_active_model(&model, configured_runtime_model.as_deref(), &run.model_id)
    });
    let context_limit_tokens = crate::app::inference_adapter::model::configured_context_length()
        .ok()
        .or(backend.context_limit_tokens)
        .or_else(|| {
            latest
                .as_ref()
                .filter(|_| latest_matches_model)
                .and_then(|run| run.context_limit_tokens)
        });
    let latest_context_tokens = latest
        .as_ref()
        .filter(|run| latest_matches_model && run.context_limit_tokens == context_limit_tokens)
        .and_then(|run| run.context_tokens_used);
    let retained_context_tokens = if adapter.fresh_session_pending {
        None
    } else {
        match context_limit_tokens {
            Some(limit) => {
                let history = adapter.conversation_memory()?.turns().to_vec();
                (!history.is_empty())
                    .then(|| {
                        let input =
                            super::super::attachment::compose_request("", &[], Some(limit)).ok()?;
                        super::super::conversation::estimate_context_tokens(
                            "", &input, &history, limit,
                        )
                        .ok()
                    })
                    .flatten()
            }
            None => None,
        }
    };
    let context_tokens_used =
        resolve_context_tokens(latest_context_tokens, retained_context_tokens);
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

fn resolve_context_tokens(observed: Option<u32>, projected: Option<u32>) -> Option<u32> {
    observed.or(projected)
}

fn same_active_model(
    selected_model: &str,
    runtime_artifact_model: Option<&str>,
    observed_model: &str,
) -> bool {
    selected_model.eq_ignore_ascii_case(observed_model)
        || runtime_artifact_model
            .is_some_and(|runtime| runtime.eq_ignore_ascii_case(observed_model))
}

#[cfg(test)]
mod tests;
