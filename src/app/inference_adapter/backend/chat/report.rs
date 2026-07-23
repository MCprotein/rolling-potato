use std::io::Write;

use crate::runtime_core::reporting::korean_guard;

use super::*;

const QWEN_NON_THINKING_SOURCE: &str =
    "https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode";
const GEMMA_NON_THINKING_SOURCE: &str = "https://ai.google.dev/gemma/docs/capabilities/thinking";

fn non_thinking_policy(model_id: &str) -> (&'static str, &'static str) {
    let model_id = model_id.to_ascii_lowercase();
    if model_id.starts_with("gemma-4") {
        return (
            "disabled via chat_template_kwargs.enable_thinking=false",
            GEMMA_NON_THINKING_SOURCE,
        );
    }
    if model_id.starts_with("qwen") {
        return (
            "disabled via chat_template_kwargs.enable_thinking=false",
            QWEN_NON_THINKING_SOURCE,
        );
    }
    (
        "best-effort system instruction",
        "model-specific source 없음",
    )
}

pub fn chat_report(
    prompt: &str,
    max_tokens: Option<u32>,
    timeout_ms: Option<u32>,
) -> Result<String, AppError> {
    let input = BackendChatInput::text_for_user(prompt, prompt);
    let run = chat_input_with_options(
        &input,
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
    let input = BackendChatInput::text_for_user(prompt, prompt);
    let direct_stream = input.response_language.allows_non_korean();
    let mut language_guard = korean_guard::StreamingGuard::default();
    let mut guarded_output = String::new();
    let mut guard_failed = false;
    writer
        .write_all(b"backend chat\n- status: streaming\n- response:\n")
        .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))?;
    writer
        .flush()
        .map_err(|err| AppError::runtime(format!("streaming output flush 실패: {err}")))?;
    let run = chat_input_with_options(
        &input,
        max_tokens,
        true,
        timeout_ms,
        || Ok(false),
        |delta| {
            if direct_stream {
                if let Some(delta) = delta {
                    writer
                        .write_all(delta.as_bytes())
                        .and_then(|_| writer.flush())
                        .map_err(|err| {
                            AppError::runtime(format!("streaming output write 실패: {err}"))
                        })?;
                }
                return Ok(());
            }
            if guard_failed {
                return Ok(());
            }
            match delta {
                Some(delta) => match language_guard.push(delta) {
                    Ok(guarded) => guarded_output.push_str(&guarded),
                    Err(_) => guard_failed = true,
                },
                None => {}
            }
            Ok(())
        },
    )?;
    if !direct_stream {
        if !guard_failed {
            match language_guard.finish() {
                Ok(guarded) => guarded_output.push_str(&guarded),
                Err(_) => guard_failed = true,
            }
        }
        let visible = if guard_failed {
            crate::app::inference_adapter::answer::fallback_visible(&run.response)?
        } else {
            guarded_output
        };
        writer
            .write_all(visible.as_bytes())
            .and_then(|_| writer.flush())
            .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))?;
    }
    writer
        .write_all(b"\n")
        .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))?;

    Ok(format_chat_run(&run, false))
}

fn format_chat_run(run: &BackendChatRun, include_response: bool) -> String {
    let (thinking_mode, non_thinking_source) = non_thinking_policy(&run.model_id);
    let mut report = format!(
        "backend chat{}\n- status: completed\n- backend: {}\n- pid: {}\n- endpoint: /v1/chat/completions\n- transport: server-sent events\n- streaming display: {}\n- thinking mode: {}\n- non-thinking source: {}\n- model id: {}\n- model path: {}\n- ctx size: {}\n- prompt chars: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- resource governor admission: {}\n- resource governor token action: {}\n- resource governor reason: {}\n- resource governor hint: {}\n- resource governor sample event: {}\n- finish reason: {}\n- guard: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- first token latency ms: {}\n- elapsed ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- ledger event: {}",
        if include_response { "" } else { " summary" },
        run.backend_id,
        run.pid,
        run.streaming_display,
        thinking_mode,
        non_thinking_source,
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
        let run = BackendChatRun::test_fixture();
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
