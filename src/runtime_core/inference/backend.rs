use std::path::PathBuf;

use crate::foundation::error::AppError;
use crate::foundation::integrity;

pub(crate) mod admission;
pub(crate) mod lifecycle;

pub(crate) const MAX_CHAT_TIMEOUT_MS: u32 = 300_000;
const MAX_CHAT_IMAGES: usize = 4;
const MAX_CHAT_IMAGE_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendChatImage {
    pub(crate) display_name: String,
    pub(crate) mime_type: String,
    pub(crate) sha256: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendChatInput {
    pub(crate) text: String,
    pub(crate) images: Vec<BackendChatImage>,
    pub(crate) response_language: ResponseLanguage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseLanguage {
    KoreanDefault,
    UserRequestedOther,
}

impl ResponseLanguage {
    pub(crate) fn from_user_request(request: &str) -> Self {
        if crate::runtime_core::reporting::korean_guard::allows_non_korean(request) {
            Self::UserRequestedOther
        } else {
            Self::KoreanDefault
        }
    }

    pub(crate) fn allows_non_korean(self) -> bool {
        matches!(self, Self::UserRequestedOther)
    }
}

impl BackendChatInput {
    pub(crate) fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            images: Vec::new(),
            response_language: ResponseLanguage::KoreanDefault,
        }
    }

    pub(crate) fn text_for_user(text: impl Into<String>, user_request: &str) -> Self {
        Self {
            text: text.into(),
            images: Vec::new(),
            response_language: ResponseLanguage::from_user_request(user_request),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), AppError> {
        if self.text.trim().is_empty() && self.images.is_empty() {
            return Err(AppError::usage(
                "backend chat은 text 또는 image 입력이 필요합니다.",
            ));
        }
        if self.images.len() > MAX_CHAT_IMAGES {
            return Err(AppError::blocked(format!(
                "이미지는 요청당 최대 {MAX_CHAT_IMAGES}개까지 사용할 수 있습니다."
            )));
        }
        let total_bytes = self.images.iter().try_fold(0_usize, |total, image| {
            total.checked_add(image.bytes.len()).ok_or_else(|| {
                AppError::blocked("이미지 입력 크기를 안전하게 계산하지 못했습니다.")
            })
        })?;
        if total_bytes > MAX_CHAT_IMAGE_BYTES {
            return Err(AppError::blocked(format!(
                "이미지 입력은 요청당 합계 {MAX_CHAT_IMAGE_BYTES} bytes를 넘을 수 없습니다."
            )));
        }
        for image in &self.images {
            let signature_matches = match image.mime_type.as_str() {
                "image/png" => image.bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
                "image/jpeg" => image.bytes.starts_with(b"\xff\xd8\xff"),
                _ => false,
            };
            if !signature_matches {
                return Err(AppError::blocked(format!(
                    "지원하지 않거나 signature가 일치하지 않는 이미지입니다: {}",
                    image.display_name
                )));
            }
            if !integrity::is_valid_sha256(&image.sha256)
                || !integrity::sha256_bytes(&image.bytes).eq_ignore_ascii_case(&image.sha256)
            {
                return Err(AppError::blocked(format!(
                    "이미지 SHA-256 검증에 실패했습니다: {}",
                    image.display_name
                )));
            }
        }
        Ok(())
    }
}

pub(crate) trait BackendAdapter {
    fn id(&self) -> &'static str;
    fn binary_name(&self) -> &'static str;
    fn managed_binary_path(&self) -> PathBuf;
    fn default_host(&self) -> &'static str;
    fn default_port(&self) -> u16;
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BackendChatSampling {
    pub(crate) temperature: f64,
    pub(crate) top_p: f64,
}

impl BackendChatSampling {
    pub(crate) fn ledger_label(&self) -> String {
        format!("temperature-{}_top-p-{}", self.temperature, self.top_p)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BackendChatRun {
    pub(crate) backend_id: String,
    pub(crate) backend_version: String,
    pub(crate) pid: u32,
    pub(crate) model_id: String,
    pub(crate) model_path: PathBuf,
    pub(crate) model_artifact_hash: String,
    pub(crate) ctx_size: Option<u32>,
    pub(crate) prompt_chars: usize,
    pub(crate) response_chars: usize,
    pub(crate) requested_max_tokens: u32,
    pub(crate) effective_max_tokens: u32,
    pub(crate) sampling: BackendChatSampling,
    pub(crate) finish_reason: String,
    pub(crate) guard_status: &'static str,
    pub(crate) prompt_tokens: Option<u32>,
    pub(crate) completion_tokens: Option<u32>,
    pub(crate) total_tokens: Option<u32>,
    pub(crate) elapsed_ms: u128,
    pub(crate) first_token_latency_ms: Option<u128>,
    pub(crate) streaming_display: bool,
    pub(crate) ledger_event: String,
    pub(crate) resource_governor_admission: String,
    pub(crate) resource_governor_token_action: String,
    pub(crate) resource_governor_reason: &'static str,
    pub(crate) resource_governor_hint: &'static str,
    pub(crate) resource_governor_sample_event: String,
    pub(crate) resource_pressure: String,
    pub(crate) resource_cpu_percent: Option<f64>,
    pub(crate) resource_average_rss_bytes: Option<u64>,
    pub(crate) resource_peak_rss_bytes: Option<u64>,
    pub(crate) resource_disk_bytes: Option<u64>,
    pub(crate) resource_sample_event: String,
    pub(crate) response: String,
}

#[cfg(test)]
impl BackendChatRun {
    pub(crate) fn test_fixture() -> Self {
        Self {
            backend_id: "llama.cpp".to_string(),
            backend_version: "b-test".to_string(),
            pid: 1234,
            model_id: "model-test".to_string(),
            model_path: std::path::PathBuf::from("/tmp/model-test.gguf"),
            model_artifact_hash: "a".repeat(64),
            ctx_size: Some(4096),
            prompt_chars: 12,
            response_chars: 5,
            requested_max_tokens: 32,
            effective_max_tokens: 16,
            sampling: BackendChatSampling {
                temperature: 0.1,
                top_p: 0.8,
            },
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_input(bytes: Vec<u8>) -> BackendChatInput {
        BackendChatInput {
            text: "이미지를 설명해줘".to_string(),
            images: vec![BackendChatImage {
                display_name: "screen.png".to_string(),
                mime_type: "image/png".to_string(),
                sha256: integrity::sha256_bytes(&bytes),
                bytes,
            }],
            response_language: ResponseLanguage::KoreanDefault,
        }
    }

    #[test]
    fn multimodal_input_requires_supported_signature_and_exact_hash() {
        let valid = png_input(b"\x89PNG\r\n\x1a\npayload".to_vec());
        assert!(valid.validate().is_ok());

        let mut tampered = valid.clone();
        tampered.images[0].bytes.push(1);
        assert!(tampered.validate().unwrap_err().message.contains("SHA-256"));

        let unsupported = BackendChatInput {
            text: "설명".to_string(),
            images: vec![BackendChatImage {
                display_name: "image.webp".to_string(),
                mime_type: "image/webp".to_string(),
                sha256: integrity::sha256_bytes(b"RIFFxxxxWEBP"),
                bytes: b"RIFFxxxxWEBP".to_vec(),
            }],
            response_language: ResponseLanguage::KoreanDefault,
        };
        assert!(unsupported
            .validate()
            .unwrap_err()
            .message
            .contains("지원하지"));
    }
}
