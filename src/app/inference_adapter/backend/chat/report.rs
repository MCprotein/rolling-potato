use std::io::Write;

use crate::runtime_core::reporting::korean_guard;

use super::*;

const QWEN_NON_THINKING_SOURCE: &str =
    "https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode";

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

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn chat_report_format_preserves_diagnostics_and_response_boundary() {
        let run = BackendChatRun {
            backend_id: "llama.cpp".to_string(),
            backend_version: "b-test".to_string(),
            pid: 1234,
            model_id: "model-test".to_string(),
            model_path: std::path::PathBuf::from("/tmp/model-test.gguf"),
            model_artifact_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            ctx_size: Some(4096),
            prompt_chars: 12,
            response_chars: 5,
            requested_max_tokens: 32,
            effective_max_tokens: 16,
            sampling: CHAT_SAMPLING,
            finish_reason: "stop".to_string(),
            guard_status: "pass",
            prompt_tokens: Some(4),
            completion_tokens: Some(2),
            total_tokens: Some(6),
            elapsed_ms: 125,
            first_token_latency_ms: Some(25),
            streaming_display: false,
            ledger_event: "chat-event".to_string(),
            resource_governor_admission: "allow".to_string(),
            resource_governor_token_action: "clamped".to_string(),
            resource_governor_reason: "degraded resource pressure",
            resource_governor_hint: "use a smaller request",
            resource_governor_sample_event: "governor-event".to_string(),
            resource_pressure: "degraded".to_string(),
            resource_cpu_percent: Some(80.0),
            resource_average_rss_bytes: Some(1024),
            resource_peak_rss_bytes: Some(2048),
            resource_disk_bytes: Some(4096),
            resource_sample_event: "sample-event".to_string(),
            response: "hello".to_string(),
        };

        let full = format_chat_run(&run, true);
        assert!(full.contains("backend chat\n- status: completed"));
        assert!(full.contains("- effective max tokens: 16"));
        assert!(full.contains("- resource governor token action: clamped"));
        assert!(full.ends_with("- response:\nhello"));

        let summary = format_chat_run(&run, false);
        assert!(summary.starts_with("backend chat summary\n- status: completed"));
        assert!(!summary.contains("- response:"));
        assert!(!summary.contains("hello"));
    }
}
