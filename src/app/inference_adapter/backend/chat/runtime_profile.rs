//! Resolve request behavior from an exact model artifact binding.

use crate::runtime_core::inference::backend::{
    lifecycle::BackendSidecarRecord, BackendChatRuntimeProfile, BackendChatSampling,
};
use crate::runtime_core::inference::model::manifest::{
    generation_profile_for_artifact_hash, ModelThinkingControl,
};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ResolvedChatRuntimeProfile {
    pub(super) model_id: String,
    pub(super) request: BackendChatRuntimeProfile,
}

pub(super) fn resolve(record: &BackendSidecarRecord) -> ResolvedChatRuntimeProfile {
    if let Some((candidate, profile)) = generation_profile_for_artifact_hash(&record.model_sha256) {
        let disable_thinking_via_template = matches!(
            profile.thinking_control,
            ModelThinkingControl::ChatTemplateEnableThinkingFalse
        );
        return ResolvedChatRuntimeProfile {
            model_id: candidate.id.to_string(),
            request: BackendChatRuntimeProfile {
                sampling_profile_version: profile
                    .sampling
                    .map(|sampling| sampling.profile_version)
                    .unwrap_or("model-default")
                    .to_string(),
                sampling: profile.sampling.map(|sampling| BackendChatSampling {
                    temperature: sampling.temperature,
                    top_p: sampling.top_p,
                }),
                disable_thinking_via_template,
                thinking_mode: if disable_thinking_via_template {
                    "disabled via source-backed chat template option".to_string()
                } else {
                    "model-default".to_string()
                },
                thinking_source: profile.thinking_source.source.to_string(),
            },
        };
    }

    ResolvedChatRuntimeProfile {
        model_id: super::super::model_identity(record),
        request: BackendChatRuntimeProfile {
            sampling_profile_version: "unregistered-model-default-v1".to_string(),
            sampling: None,
            disable_thinking_via_template: false,
            thinking_mode: "best-effort system instruction".to_string(),
            thinking_source: "unregistered artifact; model-specific source unavailable".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn record(hash: &str, path: &str) -> BackendSidecarRecord {
        BackendSidecarRecord {
            backend_id: "llama.cpp".to_string(),
            pid: 42,
            binary_path: PathBuf::from("/tmp/llama-server"),
            model_path: PathBuf::from(path),
            model_sha256: hash.to_string(),
            model_size_bytes: 1,
            backend_release: "test".to_string(),
            binary_sha256: "b".repeat(64),
            mmproj: "not-required".to_string(),
            mmproj_path: None,
            mmproj_sha256: None,
            mmproj_size_bytes: None,
            host: "127.0.0.1".to_string(),
            port: 8080,
            ctx_size: Some(4_096),
            stdout_log: PathBuf::from("/tmp/stdout"),
            stderr_log: PathBuf::from("/tmp/stderr"),
            started_at_ms: 1,
        }
    }

    #[test]
    fn exact_artifact_hash_resolves_source_backed_request_behavior() {
        let qwen = resolve(&record(
            "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
            "/tmp/arbitrary-name.gguf",
        ));

        assert_eq!(qwen.model_id, "qwen3.5-4b");
        assert!(qwen.request.disable_thinking_via_template);
        assert!(qwen.request.sampling.is_none());
        assert_eq!(qwen.request.sampling_profile_version, "model-default");
        assert!(qwen.request.thinking_source.starts_with("https://"));
    }

    #[test]
    fn filename_cannot_activate_model_specific_behavior() {
        let unknown = resolve(&record(&"f".repeat(64), "/tmp/qwen3.5-4b.gguf"));

        assert_eq!(
            unknown.model_id,
            format!("unregistered-artifact:{}", "f".repeat(64))
        );
        assert!(!unknown.request.disable_thinking_via_template);
        assert!(unknown.request.sampling.is_none());
        assert!(unknown
            .request
            .thinking_source
            .contains("unregistered artifact"));
    }
}
