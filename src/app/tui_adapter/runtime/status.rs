use super::backend::vision_status;
use super::TuiRuntimeAdapter;
use crate::foundation::error::AppError;
use crate::surfaces::tui::runtime_bridge::{
    TuiAttachment, TuiAttachmentKind, TuiBackendStatus, TuiStatusSnapshot,
};

pub(super) fn estimate_context_tokens(
    adapter: &mut TuiRuntimeAdapter,
    request: &str,
    attachments: &[TuiAttachment],
) -> Option<u32> {
    let limit = crate::app::inference_adapter::model::configured_context_length().ok()?;
    if adapter.fresh_session_pending {
        return Some(estimate_retained_tokens(&[], request, attachments, limit));
    }
    Some(estimate_retained_tokens(
        adapter.conversation_memory().ok()?.turns(),
        request,
        attachments,
        limit,
    ))
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
                let history = adapter.conversation_memory()?.turns();
                (!history.is_empty()).then(|| estimate_retained_tokens(history, "", &[], limit))
            }
            None => None,
        }
    };
    let context_tokens_used = retained_context_tokens.or(latest_context_tokens);
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

fn estimate_retained_tokens(
    history: &[crate::surfaces::tui::runtime_bridge::TuiConversationTurn],
    request: &str,
    attachments: &[TuiAttachment],
    limit: u32,
) -> u32 {
    let mut estimated =
        crate::runtime_core::knowledge::compaction::estimate_tokens(request).saturating_add(256);
    estimated = history.iter().fold(estimated, |total, turn| {
        total
            .saturating_add(crate::runtime_core::knowledge::compaction::estimate_tokens(
                &turn.content,
            ))
            .saturating_add(8)
    });
    estimated = attachments.iter().fold(estimated, |total, attachment| {
        let attachment_estimate = match attachment.kind {
            TuiAttachmentKind::Text => {
                usize::try_from(attachment.size_bytes / 3).unwrap_or(usize::MAX)
            }
            TuiAttachmentKind::Image => 256,
        };
        total.saturating_add(attachment_estimate)
    });
    u32::try_from(estimated).unwrap_or(u32::MAX).min(limit)
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
mod tests {
    use super::*;

    #[test]
    fn configured_manifest_id_matches_its_backend_artifact_stem() {
        assert!(same_active_model(
            "gemma-4-e4b",
            Some("gemma-4-E4B_q4_0-it"),
            "gemma-4-E4B_q4_0-it"
        ));
        assert!(same_active_model(
            "qwen3.5-4b",
            Some("Qwen3.5-4B-Q4_K_M"),
            "Qwen3.5-4B-Q4_K_M"
        ));
        assert!(!same_active_model(
            "qwen3.5-4b",
            Some("Qwen3.5-4B-Q4_K_M"),
            "gemma-4-E4B_q4_0-it"
        ));
    }

    #[test]
    fn retained_context_grows_for_success_and_runtime_error_turns() {
        use crate::surfaces::tui::runtime_bridge::{TuiConversationRole, TuiConversationTurn};

        let successful = vec![
            TuiConversationTurn {
                role: TuiConversationRole::User,
                content: "첫 질문".to_string(),
            },
            TuiConversationTurn {
                role: TuiConversationRole::Assistant,
                content: "첫 답변".to_string(),
            },
        ];
        let mut with_failure = successful.clone();
        with_failure.extend([
            TuiConversationTurn {
                role: TuiConversationRole::User,
                content: "검색해줘".to_string(),
            },
            TuiConversationTurn {
                role: TuiConversationRole::Error,
                content: "웹 검색 근거를 찾지 못했습니다.".to_string(),
            },
        ]);

        let first = estimate_retained_tokens(&successful, "", &[], 262_144);
        let second = estimate_retained_tokens(&with_failure, "", &[], 262_144);

        assert!(first > 256);
        assert!(second > first);
    }
}
