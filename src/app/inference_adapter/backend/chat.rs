use std::time::{Duration, Instant};

use crate::adapters::filesystem::backend_state;
use crate::adapters::llama_cpp::backend as llama_backend;
use crate::adapters::llama_cpp::stream as backend_stream;
use crate::adapters::process::backend as backend_process;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;
use crate::runtime_core::inference::backend::{
    BackendChatInput, BackendChatRun, BackendChatSampling, MAX_CHAT_TIMEOUT_MS,
};
use crate::runtime_core::inference::model::manifest::quantization_for_artifact_hash;
use crate::runtime_core::inference::{resource, stream::StreamTermination};

use super::generation_state::{
    begin_active_generation, generation_cancel_requested, write_generation_terminal_record,
    ActiveGenerationGuard,
};
use super::resource_sampling::{
    display_optional_f64, display_optional_u64_unknown, record_backend_resource_sample,
};
use super::sidecar::trace_backend_start;
use super::{
    display_optional_u128, display_optional_u32, model_id_from_path, now_ms, HEALTH_TIMEOUT_MS,
};

mod interruption;
mod report;
pub use interruption::cancel_generation_report;
use interruption::{finish_interrupted_generation, GenerationTerminalContext};
pub use report::{chat_report, chat_stream_report};

const CHAT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_CHAT_MAX_TOKENS: u32 = 128;
const CHAT_SAMPLING: BackendChatSampling = BackendChatSampling {
    temperature: 0.1,
    top_p: 0.8,
};
pub fn chat_once(prompt: &str, max_tokens: Option<u32>) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(prompt, max_tokens, false, None, || Ok(false), |_| Ok(()))
}

pub(crate) fn chat_once_with_input(
    input: &BackendChatInput,
    max_tokens: Option<u32>,
) -> Result<BackendChatRun, AppError> {
    chat_input_with_options(input, max_tokens, false, None, || Ok(false), |_| Ok(()))
}

pub fn chat_once_bounded(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(
        prompt,
        Some(max_tokens),
        false,
        Some(timeout_ms),
        || Ok(false),
        |_| Ok(()),
    )
}

pub fn chat_once_bounded_with_cancel(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
    cancel_requested: impl FnMut() -> Result<bool, AppError>,
) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(
        prompt,
        Some(max_tokens),
        false,
        Some(timeout_ms),
        cancel_requested,
        |_| Ok(()),
    )
}

pub fn preflight_chat_ready() -> Result<(), AppError> {
    ready_sidecar_record().map(|_| ())
}

fn ready_sidecar_record() -> Result<BackendSidecarRecord, AppError> {
    let Some(record) = backend_state::read_sidecar_record()? else {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: 실행 중인 sidecar record가 없습니다.\n- 다음 단계: rpotato backend start --model <path> --ctx-size 4096\n- sidecar record: {}",
            backend_state::sidecar_record_path().display()
        )));
    };
    if !backend_process::is_running(record.pid) {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: sidecar record는 있지만 process가 실행 중이 아닙니다.\n- pid: {}\n- 다음 단계: rpotato backend stop으로 stale record를 정리한 뒤 다시 시작하세요.",
            record.pid
        )));
    }

    let health = llama_backend::probe_health(
        &record.host,
        record.port,
        Duration::from_millis(HEALTH_TIMEOUT_MS),
    );
    if health.status != "healthy" {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: sidecar health check 실패\n- pid: {}\n- health: {}\n- health error: {}\n- 다음 단계: rpotato backend status로 log path를 확인하세요.",
            record.pid,
            health.status,
            health.error.unwrap_or_else(|| "없음".to_string())
        )));
    }
    Ok(record)
}

fn chat_once_with_options(
    prompt: &str,
    max_tokens: Option<u32>,
    streaming_display: bool,
    timeout_ms: Option<u32>,
    mut external_cancel_requested: impl FnMut() -> Result<bool, AppError>,
    mut on_delta: impl FnMut(Option<&str>) -> Result<(), AppError>,
) -> Result<BackendChatRun, AppError> {
    let input = BackendChatInput::text(prompt);
    chat_input_with_options(
        &input,
        max_tokens,
        streaming_display,
        timeout_ms,
        &mut external_cancel_requested,
        &mut on_delta,
    )
}

fn chat_input_with_options(
    input: &BackendChatInput,
    max_tokens: Option<u32>,
    streaming_display: bool,
    timeout_ms: Option<u32>,
    mut external_cancel_requested: impl FnMut() -> Result<bool, AppError>,
    mut on_delta: impl FnMut(Option<&str>) -> Result<(), AppError>,
) -> Result<BackendChatRun, AppError> {
    input.validate()?;
    let requested_max_tokens = max_tokens.unwrap_or(DEFAULT_CHAT_MAX_TOKENS);
    let record = ready_sidecar_record()?;
    if !input.images.is_empty() && record.mmproj_path.is_none() {
        return Err(AppError::blocked(
            "이미지 입력을 사용할 수 없습니다.\n- 이유: 현재 backend는 text-ready이지만 vision-ready가 아닙니다.\n- 다음: /model에서 vision(mmproj) 준비 상태를 확인한 뒤 모델을 다시 준비하세요.",
        ));
    }

    let governor_sample = record_backend_resource_sample(&record, "chat-governor")?;
    let governor = resource::chat_governor_decision(governor_sample.pressure, requested_max_tokens);
    if governor.is_blocked() {
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
        return Err(AppError::blocked(format!(
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
        )));
    }
    let effective_max_tokens = governor
        .effective_max_tokens
        .unwrap_or(requested_max_tokens);

    let timeout_ms = timeout_ms.unwrap_or(CHAT_TIMEOUT_MS as u32);
    if timeout_ms == 0 || timeout_ms > MAX_CHAT_TIMEOUT_MS {
        return Err(AppError::usage(format!(
            "backend chat timeout은 1..={MAX_CHAT_TIMEOUT_MS} ms 범위여야 합니다."
        )));
    }
    let generation = begin_active_generation(&record, timeout_ms, streaming_display)?;
    let generation_guard = ActiveGenerationGuard {
        generation_id: generation.generation_id.clone(),
        finished: false,
    };
    let started_event = state::record_event(
        "backend.generation.started",
        "backend generation 시작",
        &format!(
            "generation_id={} client_pid={} sidecar_pid={} backend={} model_id={} prompt_chars={} requested_max_tokens={} effective_max_tokens={} timeout_ms={} transport=sse streaming_display={} resource_governor_sample_event={}",
            generation.generation_id,
            generation.client_pid,
            generation.sidecar_pid,
            record.backend_id,
            model_id_from_path(&record.model_path),
            input.text.chars().count(),
            requested_max_tokens,
            effective_max_tokens,
            timeout_ms,
            streaming_display,
            governor_sample.ledger_event
        ),
    )?;
    let started_at_ms = now_ms();
    let started_at = Instant::now();
    let sampling = CHAT_SAMPLING;
    let body = llama_backend::chat_request_body_for_input(
        &record.model_path,
        input,
        effective_max_tokens,
        &sampling,
        true,
    );
    let stream_outcome = backend_stream::post_chat_stream(
        &record.host,
        record.port,
        "/v1/chat/completions",
        &body,
        Duration::from_millis(u64::from(timeout_ms)),
        || {
            if generation_cancel_requested(&generation.generation_id)? {
                return Ok(true);
            }
            external_cancel_requested()
        },
        |delta| on_delta(Some(delta)),
    );
    let stream_outcome = match stream_outcome {
        Ok(outcome) if outcome.termination == StreamTermination::Completed => {
            on_delta(None).map(|()| outcome)
        }
        other => other,
    };
    let elapsed_ms = started_at.elapsed().as_millis();
    let outcome = match stream_outcome {
        Ok(outcome) => outcome,
        Err(err) => {
            trace_backend_start(&format!(
                "generation-failed code={} message={}",
                err.code,
                err.message.replace('\n', " | ")
            ));
            let event_id = state::record_event(
                "backend.generation.failed",
                "backend generation 실패",
                &format!(
                    "generation_id={} sidecar_pid={} started_event={} timeout_ms={} elapsed_ms={} error_code={} error_detail=redacted",
                    generation.generation_id,
                    record.pid,
                    started_event,
                    timeout_ms,
                    elapsed_ms, err.code
                ),
            )?;
            write_generation_terminal_record(&generation.generation_id, "failed", &event_id)?;
            let resource_sample = record_backend_resource_sample(&record, "chat-failed")?;
            let identity = ledger::validated_current_identity()?;
            observability::record_model_run(&observability::ModelRunMetric {
                model_run_id: format!("model-run-{event_id}"),
                session_id: identity.session_id,
                workflow_id: None,
                model_id: model_id_from_path(&record.model_path),
                model_artifact_hash: Some(record.model_sha256.clone()),
                backend_id: Some(record.backend_id.clone()),
                backend_version: Some(record.backend_release.clone()),
                quantization: quantization_for_artifact_hash(&record.model_sha256)
                    .map(str::to_string),
                context_limit_tokens: record.ctx_size,
                started_at_ms,
                first_token_latency_ms: None,
                total_latency_ms: Some(elapsed_ms as f64),
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
                max_output_tokens: Some(effective_max_tokens),
            })?;
            generation_guard.finish()?;
            return Err(AppError {
                code: err.code,
                message: format!(
                    "{}\n- resource sample event: {}\n- lifecycle event: {event_id}",
                    err.message, resource_sample.ledger_event
                ),
            });
        }
    };
    if outcome.termination != StreamTermination::Completed {
        let interrupted = finish_interrupted_generation(
            &record,
            &generation,
            &outcome,
            GenerationTerminalContext {
                started_event,
                started_at_ms,
                elapsed_ms,
                requested_max_tokens,
                effective_max_tokens,
            },
        );
        generation_guard.finish()?;
        return interrupted;
    }

    let completion = outcome.completion;
    let display_content = completion.content.trim().to_string();
    let guard_status = if completion.had_reasoning_trace {
        if display_content.is_empty() {
            "blocked-empty-after-reasoning-strip"
        } else {
            "stripped-reasoning-trace"
        }
    } else {
        "pass"
    };
    let event_type = if display_content.is_empty() {
        "backend.chat.guard.blocked"
    } else {
        "backend.chat.completed"
    };
    let event_id = state::record_event(
        event_type,
        "backend chat completion 실행",
        &format!(
            "generation_id={} started_event={} pid={} backend={} backend_release={} binary_sha256={} model_id={} model_sha256={} model_size_bytes={} ctx_size={} mmproj={} sampling={} host_os={} host_arch={} prompt_chars={} output_chars={} requested_max_tokens={} effective_max_tokens={} timeout_ms={} transport=sse streaming_display={} resource_governor_admission={} resource_governor_token_action={} resource_governor_reason={} resource_governor_sample_event={} finish_reason={} guard_status={} prompt_tokens={} completion_tokens={} total_tokens={} first_token_latency_ms={} elapsed_ms={}",
            generation.generation_id,
            started_event,
            record.pid,
            record.backend_id,
            record.backend_release,
            record.binary_sha256,
            model_id_from_path(&record.model_path),
            record.model_sha256,
            record.model_size_bytes,
            display_optional_u32(record.ctx_size),
            record.mmproj,
            sampling.ledger_label(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            input.text.chars().count(),
            display_content.chars().count(),
            requested_max_tokens,
            effective_max_tokens,
            timeout_ms,
            streaming_display,
            governor.admission.as_str(),
            governor.token_action.as_str(),
            governor.reason,
            governor_sample.ledger_event,
            completion.finish_reason,
            guard_status,
            display_optional_u32(completion.prompt_tokens),
            display_optional_u32(completion.completion_tokens),
            display_optional_u32(completion.total_tokens),
            display_optional_u128(completion.first_token_latency_ms),
            elapsed_ms
        ),
    )?;

    write_generation_terminal_record(&generation.generation_id, "completed", &event_id)?;
    let resource_sample = record_backend_resource_sample(
        &record,
        if streaming_display {
            "chat-stream"
        } else {
            "chat"
        },
    )?;

    let identity = ledger::validated_current_identity()?;
    let model_id = model_id_from_path(&record.model_path);
    let model_run_id = format!("model-run-{event_id}");
    let completion_tokens = completion.completion_tokens.unwrap_or(0);
    let tokens_per_second = if completion_tokens > 0 && elapsed_ms > 0 {
        Some((completion_tokens as f64) / ((elapsed_ms as f64) / 1000.0))
    } else {
        None
    };
    observability::record_model_run(&observability::ModelRunMetric {
        model_run_id,
        session_id: identity.session_id,
        workflow_id: None,
        model_id: model_id.clone(),
        model_artifact_hash: Some(record.model_sha256.clone()),
        backend_id: Some(record.backend_id.clone()),
        backend_version: Some(record.backend_release.clone()),
        quantization: quantization_for_artifact_hash(&record.model_sha256).map(str::to_string),
        context_limit_tokens: record.ctx_size,
        started_at_ms,
        first_token_latency_ms: completion.first_token_latency_ms.map(|value| value as f64),
        total_latency_ms: Some(elapsed_ms as f64),
        prompt_eval_ms: None,
        generation_eval_ms: None,
        tokens_per_second,
        cancelled: false,
        token_usage_complete: completion.prompt_tokens.is_some()
            && completion.completion_tokens.is_some()
            && completion.total_tokens.is_some(),
        prompt_tokens: completion.prompt_tokens.unwrap_or(0),
        completion_tokens,
        total_tokens: completion.total_tokens.unwrap_or(0),
        context_tokens_used: completion.prompt_tokens.unwrap_or(0),
        context_tokens_dropped: 0,
        ontology_tokens: 0,
        tool_summary_tokens: 0,
        max_output_tokens: Some(effective_max_tokens),
    })?;
    if display_content.is_empty() {
        generation_guard.finish()?;
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: reasoning trace 제거 후 표시 가능한 응답이 없습니다.\n- endpoint: /v1/chat/completions\n- thinking mode: disabled via chat_template_kwargs.enable_thinking=false\n- guard: {}\n- finish reason: {}\n- resource sample event: {}\n- lifecycle event: {}",
            guard_status,
            completion.finish_reason,
            resource_sample.ledger_event,
            event_id
        )));
    }

    let run = BackendChatRun {
        backend_id: record.backend_id,
        backend_version: record.backend_release,
        pid: record.pid,
        model_id,
        model_path: record.model_path,
        model_artifact_hash: record.model_sha256,
        ctx_size: record.ctx_size,
        prompt_chars: input.text.chars().count(),
        response_chars: display_content.chars().count(),
        requested_max_tokens,
        effective_max_tokens,
        sampling,
        finish_reason: completion.finish_reason,
        guard_status,
        prompt_tokens: completion.prompt_tokens,
        completion_tokens: completion.completion_tokens,
        total_tokens: completion.total_tokens,
        elapsed_ms,
        first_token_latency_ms: completion.first_token_latency_ms,
        streaming_display,
        ledger_event: event_id,
        resource_governor_admission: governor.admission.as_str().to_string(),
        resource_governor_token_action: governor.token_action.as_str().to_string(),
        resource_governor_reason: governor.reason,
        resource_governor_hint: governor.hint,
        resource_governor_sample_event: governor_sample.ledger_event,
        resource_pressure: resource_sample.metric.pressure_status,
        resource_cpu_percent: resource_sample.metric.process_cpu_percent,
        resource_average_rss_bytes: resource_sample.metric.average_rss_bytes,
        resource_peak_rss_bytes: resource_sample.metric.peak_rss_bytes,
        resource_disk_bytes: resource_sample.metric.disk_bytes,
        resource_sample_event: resource_sample.ledger_event,
        response: display_content,
    };
    generation_guard.finish()?;
    Ok(run)
}
