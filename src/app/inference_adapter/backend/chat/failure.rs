use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::lifecycle::{
    BackendGenerationRecord, BackendSidecarRecord,
};
use crate::runtime_core::inference::backend::{BackendChatInput, BackendChatRun};
use crate::runtime_core::inference::model::manifest::quantization_for_artifact_hash;
use crate::runtime_core::inference::resource::ResourceGovernorDecision;

use super::super::generation_state::write_generation_terminal_record;
use super::super::resource_sampling::{
    record_backend_resource_sample, BackendResourceSampleReport,
};
use super::super::sidecar::trace_backend_start;

pub(super) struct StreamFailureContext<'a> {
    pub(super) record: &'a BackendSidecarRecord,
    pub(super) generation: &'a BackendGenerationRecord,
    pub(super) started_event: &'a str,
    pub(super) total_timeout_ms: u32,
    pub(super) elapsed_ms: u128,
    pub(super) started_at_ms: u128,
    pub(super) model_id: &'a str,
    pub(super) effective_max_tokens: u32,
}

pub(super) fn finish_preflight_failure(
    generation: &BackendGenerationRecord,
    error: AppError,
    cancelled: bool,
    timed_out: bool,
    phase: &str,
    elapsed_ms: u128,
) -> Result<BackendChatRun, AppError> {
    let (event_type, outcome, status_label) = if cancelled {
        (
            "backend.generation.cancelled",
            "cancelled",
            "사용자 요청으로 취소됨",
        )
    } else if timed_out {
        (
            "backend.generation.timeout",
            "timed-out",
            "제한 시간 초과로 취소됨",
        )
    } else {
        ("backend.generation.failed", "failed", "preflight 실패")
    };
    let event_id = state::record_event(
        event_type,
        "backend generation preflight 종료",
        &format!(
            "generation_id={} client_pid={} sidecar_pid={} status={} phase={} timeout_ms={} elapsed_ms={} error_code={} error_detail=redacted",
            generation.generation_id,
            generation.client_pid,
            generation.sidecar_pid,
            outcome,
            phase,
            generation.timeout_ms,
            elapsed_ms,
            error.code
        ),
    )?;
    write_generation_terminal_record(&generation.generation_id, outcome, &event_id)?;
    Err(AppError {
        code: error.code,
        message: format!(
            "backend chat 중단\n- 상태: {status_label}\n- generation id: {}\n- sidecar pid: {}\n- phase: {}\n- 경과 시간 ms: {}\n- 원인: {}\n- lifecycle event: {}",
            generation.generation_id,
            generation.sidecar_pid,
            phase,
            elapsed_ms,
            error.message,
            event_id
        ),
    })
}

pub(super) fn finish_stream_failure(
    error: AppError,
    context: StreamFailureContext<'_>,
) -> Result<BackendChatRun, AppError> {
    trace_backend_start(&format!(
        "generation-failed code={} message={}",
        error.code,
        error.message.replace('\n', " | ")
    ));
    let event_id = state::record_event(
        "backend.generation.failed",
        "backend generation 실패",
        &format!(
            "generation_id={} sidecar_pid={} started_event={} timeout_ms={} elapsed_ms={} error_code={} error_detail=redacted",
            context.generation.generation_id,
            context.record.pid,
            context.started_event,
            context.total_timeout_ms,
            context.elapsed_ms,
            error.code
        ),
    )?;
    write_generation_terminal_record(&context.generation.generation_id, "failed", &event_id)?;
    let resource_sample = record_backend_resource_sample(context.record, "chat-failed")?;
    let identity = ledger::validated_current_identity()?;
    observability::record_model_run(&observability::ModelRunMetric {
        model_run_id: format!("model-run-{event_id}"),
        session_id: identity.session_id,
        workflow_id: None,
        model_id: context.model_id.to_string(),
        model_artifact_hash: Some(context.record.model_sha256.clone()),
        backend_id: Some(context.record.backend_id.clone()),
        backend_version: Some(context.record.backend_release.clone()),
        quantization: quantization_for_artifact_hash(&context.record.model_sha256)
            .map(str::to_string),
        context_limit_tokens: context.record.ctx_size,
        started_at_ms: context.started_at_ms,
        first_token_latency_ms: None,
        total_latency_ms: Some(context.elapsed_ms as f64),
        prompt_eval_ms: None,
        generation_eval_ms: None,
        tokens_per_second: None,
        cancelled: false,
        token_usage_complete: false,
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        context_tokens_used: 0,
        context_tokens_dropped: 0,
        ontology_tokens: 0,
        tool_summary_tokens: 0,
        max_output_tokens: Some(context.effective_max_tokens),
    })?;
    Err(AppError {
        code: error.code,
        message: format!(
            "{}\n- resource sample event: {}\n- lifecycle event: {event_id}",
            error.message, resource_sample.ledger_event
        ),
    })
}

pub(super) fn resource_governor_blocked(
    input: &BackendChatInput,
    record: &BackendSidecarRecord,
    governor_sample: &BackendResourceSampleReport,
    governor: &ResourceGovernorDecision,
    requested_max_tokens: Option<u32>,
) -> Result<BackendChatRun, AppError> {
    let requested_max_tokens = requested_max_tokens
        .map(|tokens| tokens.to_string())
        .unwrap_or_else(|| "pending-exact-preflight".to_string());
    let event_id = state::record_event(
        "backend.chat.governor.blocked",
        "backend chat resource governor 차단",
        &format!(
            "pid={} backend={} prompt_chars={} requested_max_tokens={} pressure_status={} admission={} token_action={} reason={} sample_event={}",
            record.pid,
            record.backend_id,
            input.text.chars().count(),
            requested_max_tokens,
            governor.pressure.as_str(),
            governor.admission.as_str(),
            governor.token_action.as_str(),
            governor.reason,
            governor_sample.ledger_event
        ),
    )?;
    Err(AppError::blocked(format!(
        "backend chat 차단\n- 이유: resource governor가 critical pressure에서 요청을 차단했습니다.\n- pid: {}\n- resource pressure: {}\n- requested max tokens: {}\n- effective max tokens: blocked\n- resource governor admission: {}\n- resource governor token action: {}\n- resource governor reason: {}\n- resource governor hint: {}\n- resource governor sample event: {}\n- ledger event: {}",
        record.pid,
        governor.pressure.as_str(),
        requested_max_tokens,
        governor.admission.as_str(),
        governor.token_action.as_str(),
        governor.reason,
        governor.hint,
        governor_sample.ledger_event,
        event_id
    )))
}
