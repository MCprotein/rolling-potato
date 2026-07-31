use std::time::{Duration, Instant};

use crate::adapters::llama_cpp::backend as llama_backend;
use crate::adapters::llama_cpp::stream as backend_stream;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::{
    BackendChatInput, BackendChatRuntimeProfile, MAX_CHAT_TIMEOUT_MS,
};

#[derive(Clone, Copy)]
pub(super) struct ChatDeadline {
    started_at: Instant,
    total_timeout: Duration,
}

impl ChatDeadline {
    pub(super) fn new(total_timeout_ms: u32) -> Self {
        Self {
            started_at: Instant::now(),
            total_timeout: Duration::from_millis(u64::from(total_timeout_ms)),
        }
    }

    pub(super) fn remaining(&self, phase: &str) -> Result<Duration, AppError> {
        self.total_timeout
            .checked_sub(self.started_at.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                AppError::blocked(format!(
                    "backend chat total timeout이 {phase} 전에 만료되었습니다."
                ))
            })
    }

    pub(super) fn remaining_ms(&self, phase: &str) -> Result<u32, AppError> {
        remaining_timeout_ms(self.total_timeout, self.started_at.elapsed(), phase)
    }

    pub(super) fn elapsed_ms(&self) -> u128 {
        self.started_at.elapsed().as_millis()
    }
}

pub(super) fn validate_timeout(timeout_ms: u32) -> Result<(), AppError> {
    if timeout_ms == 0 || timeout_ms > MAX_CHAT_TIMEOUT_MS {
        return Err(AppError::usage(format!(
            "backend chat timeout은 1..={MAX_CHAT_TIMEOUT_MS} ms 범위여야 합니다."
        )));
    }
    Ok(())
}

pub(super) fn ensure_vision_ready(
    input: &BackendChatInput,
    projector_ready: bool,
) -> Result<(), AppError> {
    if !input.images.is_empty() && !projector_ready {
        return Err(AppError::blocked(
            "이미지 입력을 사용할 수 없습니다.\n- 이유: 현재 backend는 text-ready이지만 vision-ready가 아닙니다.\n- 다음: /model에서 vision(mmproj) 준비 상태를 확인한 뒤 모델을 다시 준비하세요.",
        ));
    }
    Ok(())
}

pub(super) fn count_input_tokens(
    input: &BackendChatInput,
    runtime_profile: &BackendChatRuntimeProfile,
    host: &str,
    port: u16,
    deadline: ChatDeadline,
    cancel_requested: &mut impl FnMut() -> Result<bool, AppError>,
) -> Result<u32, AppError> {
    let body = llama_backend::chat_input_tokens_request_body(input, runtime_profile)?;
    let response = backend_stream::post_bounded_json(
        host,
        port,
        "/v1/chat/completions/input_tokens",
        &body,
        deadline.remaining("input token preflight")?,
        cancel_requested,
    )?;
    llama_backend::parse_chat_input_tokens_response(&response)
}

fn remaining_timeout_ms(
    total_timeout: Duration,
    elapsed: Duration,
    phase: &str,
) -> Result<u32, AppError> {
    let remaining = total_timeout
        .checked_sub(elapsed)
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| {
            AppError::blocked(format!(
                "backend chat total timeout이 {phase} 전에 만료되었습니다."
            ))
        })?;
    u32::try_from(remaining.as_millis())
        .ok()
        .filter(|remaining_ms| *remaining_ms > 0)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "backend chat total timeout이 {phase} 전에 만료되었습니다."
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::remaining_timeout_ms;
    use std::time::Duration;

    #[test]
    fn preflight_consumes_the_single_total_chat_timeout() {
        assert_eq!(
            remaining_timeout_ms(
                Duration::from_millis(30_000),
                Duration::from_millis(4_250),
                "stream",
            )
            .unwrap(),
            25_750
        );
        assert!(remaining_timeout_ms(
            Duration::from_millis(30_000),
            Duration::from_millis(30_000),
            "stream",
        )
        .is_err());
        assert!(remaining_timeout_ms(
            Duration::from_millis(30_000),
            Duration::from_millis(30_001),
            "stream",
        )
        .is_err());
    }
}
