//! Exact input-token preflight for the pinned llama.cpp chat contract.

use crate::foundation::error::AppError;
use crate::foundation::serialization;
use crate::runtime_core::inference::backend::{BackendChatInput, BackendChatRuntimeProfile};

use super::request::chat_request_body_for_input;

const MAX_TOKENS_PLACEHOLDER: u32 = 1;

pub(crate) fn chat_input_tokens_request_body(
    input: &BackendChatInput,
    runtime_profile: &BackendChatRuntimeProfile,
) -> Result<String, AppError> {
    chat_request_body_for_input(input, MAX_TOKENS_PLACEHOLDER, runtime_profile, false)
}

pub(crate) fn parse_chat_input_tokens_response(body: &str) -> Result<u32, AppError> {
    let object = serialization::parse_object(
        body,
        &["object", "input_tokens"],
        "llama.cpp input token response",
    )?;
    let tokens = serialization::number(&object, "input_tokens", "llama.cpp input token response")?;
    u32::try_from(tokens)
        .ok()
        .filter(|tokens| *tokens > 0)
        .ok_or_else(|| {
            AppError::blocked(
                "llama.cpp input token response의 input_tokens는 1..=u32::MAX 범위여야 합니다.",
            )
        })
}
