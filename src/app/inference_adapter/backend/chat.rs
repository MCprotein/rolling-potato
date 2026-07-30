use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::{
    BackendChatInput, BackendChatRun, BackendChatSampling,
};
use crate::runtime_core::inference::generation_policy::GenerationIntent;

mod execution;
mod interruption;
mod readiness;
mod report;

use super::generation_gateway::GenerationTokenRequest;
use execution::chat_input_with_options;
pub use interruption::cancel_generation_report;
pub use report::{chat_report, chat_stream_report};

pub(super) const CHAT_TIMEOUT_MS: u64 = 30_000;
pub(super) const CHAT_SAMPLING: BackendChatSampling = BackendChatSampling {
    temperature: 0.1,
    top_p: 0.8,
};

pub fn chat_once(prompt: &str, max_tokens: Option<u32>) -> Result<BackendChatRun, AppError> {
    let request = GenerationTokenRequest::interactive_or_explicit(max_tokens);
    chat_once_with_options(prompt, request, false, None, || Ok(false), |_| Ok(()))
}

pub(crate) fn chat_once_for_intent(
    prompt: &str,
    intent: GenerationIntent,
) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(
        prompt,
        GenerationTokenRequest::Intent(intent),
        false,
        None,
        || Ok(false),
        |_| Ok(()),
    )
}

pub(crate) fn chat_once_with_input_for_intent(
    input: &BackendChatInput,
    intent: GenerationIntent,
) -> Result<BackendChatRun, AppError> {
    chat_input_with_options(
        input,
        GenerationTokenRequest::Intent(intent),
        false,
        None,
        || Ok(false),
        |_| Ok(()),
    )
}

pub fn chat_once_bounded(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(
        prompt,
        GenerationTokenRequest::ExplicitBound(max_tokens),
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
        GenerationTokenRequest::ExplicitBound(max_tokens),
        false,
        Some(timeout_ms),
        cancel_requested,
        |_| Ok(()),
    )
}

pub fn preflight_chat_ready() -> Result<(), AppError> {
    readiness::ready_sidecar_record().map(|_| ())
}

fn chat_once_with_options(
    prompt: &str,
    request: GenerationTokenRequest,
    streaming_display: bool,
    timeout_ms: Option<u32>,
    mut external_cancel_requested: impl FnMut() -> Result<bool, AppError>,
    mut on_delta: impl FnMut(Option<&str>) -> Result<(), AppError>,
) -> Result<BackendChatRun, AppError> {
    let input = BackendChatInput::text(prompt);
    chat_input_with_options(
        &input,
        request,
        streaming_display,
        timeout_ms,
        &mut external_cancel_requested,
        &mut on_delta,
    )
}
