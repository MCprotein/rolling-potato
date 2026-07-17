use std::io::Write;
use std::time::{Duration, Instant};

use crate::adapters::filesystem::backend_state;
use crate::adapters::llama_cpp::backend as llama_backend;
use crate::adapters::llama_cpp::stream as backend_stream;
use crate::adapters::process::backend as backend_process;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::lifecycle::{
    BackendGenerationRecord, BackendSidecarRecord,
};
use crate::runtime_core::inference::backend::{
    BackendChatRun, BackendChatSampling, MAX_CHAT_TIMEOUT_MS,
};
use crate::runtime_core::inference::model::manifest::quantization_for_artifact_hash;
use crate::runtime_core::inference::{
    resource,
    stream::{StreamOutcome, StreamTermination},
};
use crate::runtime_core::reporting::korean_guard;

use super::generation_state::{
    begin_active_generation, generation_cancel_requested, remove_generation_state_if_owned,
    wait_for_generation_group_release, wait_for_generation_terminal,
    write_generation_cancel_marker, write_generation_terminal_record, ActiveGenerationGuard,
};
use super::resource_sampling::{
    display_optional_f64, display_optional_u64_unknown, record_backend_resource_sample,
};
use super::sidecar::trace_backend_start;
use super::{
    display_optional_u128, display_optional_u32, model_id_from_path, now_ms, HEALTH_TIMEOUT_MS,
};

const CHAT_TIMEOUT_MS: u64 = 30_000;
const CANCEL_WAIT_MS: u64 = 2_000;
const DEFAULT_CHAT_MAX_TOKENS: u32 = 128;
const CHAT_SAMPLING: BackendChatSampling = BackendChatSampling {
    temperature: 0.1,
    top_p: 0.8,
};
const QWEN_NON_THINKING_SOURCE: &str =
    "https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode";

struct GenerationTerminalContext {
    started_event: String,
    started_at_ms: u128,
    elapsed_ms: u128,
    requested_max_tokens: u32,
    effective_max_tokens: u32,
}

pub fn chat_report(
    prompt: &str,
    max_tokens: Option<u32>,
    timeout_ms: Option<u32>,
) -> Result<String, AppError> {
    let run = chat_once_with_options(
        prompt,
        max_tokens,
        false,
        timeout_ms,
        || Ok(false),
        |_| Ok(()),
    )?;

    Ok(format_chat_run(&run, true))
}

pub fn chat_stream_report(
    prompt: &str,
    max_tokens: Option<u32>,
    timeout_ms: Option<u32>,
    writer: &mut impl Write,
) -> Result<String, AppError> {
    let mut language_guard = korean_guard::StreamingGuard::default();
    writer
        .write_all(b"backend chat\n- status: streaming\n- response:\n")
        .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))?;
    writer
        .flush()
        .map_err(|err| AppError::runtime(format!("streaming output flush 실패: {err}")))?;
    let run = chat_once_with_options(
        prompt,
        max_tokens,
        true,
        timeout_ms,
        || Ok(false),
        |delta| {
            let guarded = match delta {
                Some(delta) => language_guard.push(delta),
                None => language_guard.finish(),
            }
            .map_err(AppError::blocked)?;
            if guarded.is_empty() {
                return Ok(());
            }
            writer
                .write_all(guarded.as_bytes())
                .and_then(|_| writer.flush())
                .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))
        },
    )?;
    writer
        .write_all(b"\n")
        .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))?;

    Ok(format_chat_run(&run, false))
}

fn format_chat_run(run: &BackendChatRun, include_response: bool) -> String {
    let mut report = format!(
        "backend chat{}\n- status: completed\n- backend: {}\n- pid: {}\n- endpoint: /v1/chat/completions\n- transport: server-sent events\n- streaming display: {}\n- thinking mode: disabled via chat_template_kwargs.enable_thinking=false\n- non-thinking source: {}\n- model id: {}\n- model path: {}\n- ctx size: {}\n- prompt chars: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- resource governor admission: {}\n- resource governor token action: {}\n- resource governor reason: {}\n- resource governor hint: {}\n- resource governor sample event: {}\n- finish reason: {}\n- guard: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- first token latency ms: {}\n- elapsed ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- ledger event: {}",
        if include_response { "" } else { " summary" },
        run.backend_id,
        run.pid,
        run.streaming_display,
        QWEN_NON_THINKING_SOURCE,
        run.model_id,
        run.model_path.display(),
        display_optional_u32(run.ctx_size),
        run.prompt_chars,
        run.requested_max_tokens,
        run.effective_max_tokens,
        run.resource_governor_admission,
        run.resource_governor_token_action,
        run.resource_governor_reason,
        run.resource_governor_hint,
        run.resource_governor_sample_event,
        run.finish_reason,
        run.guard_status,
        display_optional_u32(run.prompt_tokens),
        display_optional_u32(run.completion_tokens),
        display_optional_u32(run.total_tokens),
        display_optional_u128(run.first_token_latency_ms),
        run.elapsed_ms,
        run.resource_pressure,
        display_optional_f64(run.resource_cpu_percent),
        display_optional_u64_unknown(run.resource_average_rss_bytes),
        display_optional_u64_unknown(run.resource_peak_rss_bytes),
        display_optional_u64_unknown(run.resource_disk_bytes),
        run.resource_sample_event,
        run.ledger_event
    );
    if include_response {
        report.push_str("\n- response:\n");
        report.push_str(&run.response);
    }
    report
}

pub fn chat_once(prompt: &str, max_tokens: Option<u32>) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(prompt, max_tokens, false, None, || Ok(false), |_| Ok(()))
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
    if prompt.trim().is_empty() {
        return Err(AppError::usage(
            "backend chat은 비어 있지 않은 --prompt <text> 값이 필요합니다.",
        ));
    }
    let requested_max_tokens = max_tokens.unwrap_or(DEFAULT_CHAT_MAX_TOKENS);
    let record = ready_sidecar_record()?;

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
                prompt.chars().count(),
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
            prompt.chars().count(),
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
    let body = llama_backend::chat_request_body(
        &record.model_path,
        prompt,
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
            prompt.chars().count(),
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
        prompt_chars: prompt.chars().count(),
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

fn finish_interrupted_generation(
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
