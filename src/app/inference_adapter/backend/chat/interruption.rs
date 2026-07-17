use std::time::{Duration, Instant};

use crate::adapters::filesystem::backend_state;
use crate::adapters::process::backend as backend_process;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::lifecycle::{
    BackendGenerationRecord, BackendSidecarRecord,
};
use crate::runtime_core::inference::backend::BackendChatRun;
use crate::runtime_core::inference::model::manifest::quantization_for_artifact_hash;
use crate::runtime_core::inference::stream::{StreamOutcome, StreamTermination};

use super::super::generation_state::{
    remove_generation_state_if_owned, wait_for_generation_group_release,
    wait_for_generation_terminal, write_generation_cancel_marker, write_generation_terminal_record,
};
use super::super::resource_sampling::record_backend_resource_sample;
use super::super::{display_optional_u128, display_optional_u32, model_id_from_path};

const CANCEL_WAIT_MS: u64 = 2_000;

pub(super) struct GenerationTerminalContext {
    pub(super) started_event: String,
    pub(super) started_at_ms: u128,
    pub(super) elapsed_ms: u128,
    pub(super) requested_max_tokens: u32,
    pub(super) effective_max_tokens: u32,
}

pub(super) fn finish_interrupted_generation(
    record: &BackendSidecarRecord,
    generation: &BackendGenerationRecord,
    outcome: &StreamOutcome,
    terminal: GenerationTerminalContext,
) -> Result<BackendChatRun, AppError> {
    let (event_type, status, status_label, resource_label) = match outcome.termination {
        StreamTermination::Cancelled => (
            "backend.generation.cancelled",
            "cancelled",
            "사용자 요청으로 취소됨",
            "chat-cancelled",
        ),
        StreamTermination::TimedOut => (
            "backend.generation.timeout",
            "timed-out",
            "제한 시간 초과로 취소됨",
            "chat-timeout",
        ),
        StreamTermination::Completed => {
            return Err(AppError::runtime(
                "완료된 generation을 interrupted 상태로 처리할 수 없습니다.",
            ));
        }
    };
    let completion = &outcome.completion;
    let event_id = state::record_event(
        event_type,
        "backend generation 중단",
        &format!(
            "generation_id={} started_event={} client_pid={} sidecar_pid={} status={} timeout_ms={} elapsed_ms={} output_chars={} requested_max_tokens={} effective_max_tokens={} first_token_latency_ms={} prompt_tokens={} completion_tokens={} total_tokens={}",
            generation.generation_id,
            terminal.started_event,
            generation.client_pid,
            generation.sidecar_pid,
            status,
            generation.timeout_ms,
            terminal.elapsed_ms,
            completion.content.chars().count(),
            terminal.requested_max_tokens,
            terminal.effective_max_tokens,
            display_optional_u128(completion.first_token_latency_ms),
            display_optional_u32(completion.prompt_tokens),
            display_optional_u32(completion.completion_tokens),
            display_optional_u32(completion.total_tokens)
        ),
    )?;
    write_generation_terminal_record(&generation.generation_id, status, &event_id)?;
    let resource_sample = record_backend_resource_sample(record, resource_label)?;
    let identity = ledger::validated_current_identity()?;
    observability::record_model_run(&observability::ModelRunMetric {
        model_run_id: format!("model-run-{event_id}"),
        session_id: identity.session_id,
        workflow_id: None,
        model_id: model_id_from_path(&record.model_path),
        model_artifact_hash: Some(record.model_sha256.clone()),
        backend_id: Some(record.backend_id.clone()),
        backend_version: Some(record.backend_release.clone()),
        quantization: quantization_for_artifact_hash(&record.model_sha256).map(str::to_string),
        context_limit_tokens: record.ctx_size,
        started_at_ms: terminal.started_at_ms,
        first_token_latency_ms: completion.first_token_latency_ms.map(|value| value as f64),
        total_latency_ms: Some(terminal.elapsed_ms as f64),
        prompt_eval_ms: None,
        generation_eval_ms: None,
        tokens_per_second: None,
        cancelled: true,
        token_usage_complete: completion.prompt_tokens.is_some()
            && completion.completion_tokens.is_some()
            && completion.total_tokens.is_some(),
        prompt_tokens: completion.prompt_tokens.unwrap_or(0),
        completion_tokens: completion.completion_tokens.unwrap_or(0),
        total_tokens: completion.total_tokens.unwrap_or(0),
        context_tokens_used: completion.prompt_tokens.unwrap_or(0),
        context_tokens_dropped: 0,
        ontology_tokens: 0,
        tool_summary_tokens: 0,
        max_output_tokens: Some(terminal.effective_max_tokens),
    })?;
    Err(AppError::runtime(format!(
        "backend chat 중단\n- 상태: {status_label}\n- generation id: {}\n- sidecar pid: {}\n- 경과 시간 ms: {}\n- 부분 출력 문자 수: {}\n- resource sample event: {}\n- lifecycle event: {}\n- sidecar 동작: 계속 실행",
        generation.generation_id,
        generation.sidecar_pid,
        terminal.elapsed_ms,
        completion.content.chars().count(),
        resource_sample.ledger_event,
        event_id
    )))
}

pub fn cancel_generation_report() -> Result<String, AppError> {
    let Some(record) = backend_state::read_generation_record()? else {
        return Ok(format!(
            "backend generation 취소\n- status: idle\n- active generation record: {}",
            backend_state::generation_record_path().display()
        ));
    };
    if !backend_process::is_running(record.client_pid) {
        remove_generation_state_if_owned(&record.generation_id);
        let event_id = state::record_event(
            "backend.generation.stale.cleaned",
            "stale backend generation record 정리",
            &format!(
                "generation_id={} client_pid={} sidecar_pid={} reason=client-not-running",
                record.generation_id, record.client_pid, record.sidecar_pid
            ),
        )?;
        return Ok(format!(
            "backend generation 취소\n- status: stale-record-cleaned\n- generation id: {}\n- client pid: {}\n- sidecar pid: {}\n- sidecar action: kept-running\n- ledger event: {}",
            record.generation_id, record.client_pid, record.sidecar_pid, event_id
        ));
    }

    write_generation_cancel_marker(&record.generation_id)?;
    let event_id = state::record_event(
        "backend.generation.cancel.requested",
        "backend generation 취소 요청",
        &format!(
            "generation_id={} client_pid={} sidecar_pid={} transport=cancel-marker sidecar_action=kept-running",
            record.generation_id, record.client_pid, record.sidecar_pid
        ),
    )?;

    let wait_started = Instant::now();
    let terminal =
        wait_for_generation_terminal(&record.generation_id, Duration::from_millis(CANCEL_WAIT_MS))?;
    let remaining = Duration::from_millis(CANCEL_WAIT_MS).saturating_sub(wait_started.elapsed());
    let group_released = if terminal.is_some() {
        wait_for_generation_group_release(&record.generation_id, remaining)?
    } else {
        false
    };
    if group_released {
        backend_state::remove_generation_terminal_record(&record.generation_id)?;
    }
    let terminal_outcome = terminal
        .as_ref()
        .map(|record| record.outcome.as_str())
        .unwrap_or("pending");
    let terminal_event = terminal
        .as_ref()
        .map(|record| record.lifecycle_event.as_str())
        .unwrap_or("not-acknowledged");

    Ok(format!(
        "backend generation 취소\n- status: {}\n- terminal outcome: {}\n- generation id: {}\n- client pid: {}\n- sidecar pid: {}\n- wait ms: {}\n- sidecar action: kept-running\n- terminal lifecycle event: {}\n- request ledger event: {}",
        if terminal.is_some() && group_released { "acknowledged" } else { "requested" },
        terminal_outcome,
        record.generation_id,
        record.client_pid,
        record.sidecar_pid,
        wait_started.elapsed().as_millis(),
        terminal_event,
        event_id
    ))
}
