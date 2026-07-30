#[path = "manifest/types.rs"]
mod types;
#[path = "manifest/validation.rs"]
mod validation;

#[allow(unused_imports)]
pub(crate) use types::*;
pub(crate) use validation::{
    find_candidate, generation_profile_for_artifact_hash, quantization_for_artifact_hash,
    source_backed_artifact, source_backed_artifact_blockers, source_backed_vision_projector,
    validate_install_ready,
};

pub(crate) const STATUS_SCHEMA: &[CandidateStatus] = &[
    CandidateStatus::Candidate,
    CandidateStatus::Unverified,
    CandidateStatus::Verified,
];

pub(crate) const QWEN_4B_BLOCKERS: &[&str] = &[
    "정적 manifest에는 host-local promotion evidence가 내장되지 않음",
    "16 GB runtime fit 미측정",
    "host-local promotion evidence 없이는 설치 불가",
];
pub(crate) const GEMMA_4B_BLOCKERS: &[&str] = &[
    "정적 manifest에는 host-local promotion evidence가 내장되지 않음",
    "16 GB runtime fit 미측정",
    "host-local promotion evidence 없이는 설치 불가",
];
pub(crate) const QWEN_9B_BLOCKERS: &[&str] = &["제품 기본값 보류", "16 GB runtime fit 미측정"];

pub(crate) const CANDIDATES: &[ModelManifestEntry] = &[
    ModelManifestEntry {
        id: "qwen3.5-4b",
        display_name: "Qwen3.5 4B Q4_K_M GGUF",
        status: CandidateStatus::Unverified,
        role: "우선 평가 후보",
        upstream_model: "Qwen/Qwen3.5-4B",
        upstream_url: "https://huggingface.co/Qwen/Qwen3.5-4B",
        format: "gguf",
        backend: "llama.cpp",
        license: SourceClaim {
            claim: "Hugging Face model card license field is apache-2.0.",
            source: "https://huggingface.co/api/models/Qwen/Qwen3.5-4B",
            checked_at: "2026-07-06",
            status: "confirmed",
        },
        artifact_provider: Some("unsloth/Qwen3.5-4B-GGUF"),
        artifact_url: Some("https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/e87f176479d0855a907a41277aca2f8ee7a09523/Qwen3.5-4B-Q4_K_M.gguf"),
        artifact_terms_url: Some("https://huggingface.co/unsloth/Qwen3.5-4B-GGUF"),
        artifact_name: Some("Qwen3.5-4B-Q4_K_M.gguf"),
        quantization: Some("Q4_K_M"),
        sha256: Some("00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4"),
        size_bytes: Some(2_740_937_888),
        vision_projector: Some(ModelArtifactDescriptor {
            provider: "unsloth/Qwen3.5-4B-GGUF",
            url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/e87f176479d0855a907a41277aca2f8ee7a09523/mmproj-F16.gguf",
            terms_url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF",
            file_name: "mmproj-F16.gguf",
            sha256: "cd88edcf8d031894960bb0c9c5b9b7e1fea6ebee02b9f7ce925a00d12891f864",
            size_bytes: 672_423_616,
        }),
        context_length: Some(262_144),
        recommended_ram_gb: None,
        backend_compatibility: Some(SourceClaim {
            claim: "Hugging Face API lists this artifact as GGUF with architecture qwen35 and endpoints_compatible; compatibility still requires host-local promotion evidence.",
            source: "https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF",
            checked_at: "2026-07-06",
            status: "source-listed-unverified",
        }),
        generation_profile: Some(ModelGenerationProfile {
            sampling: ModelSamplingProfile {
                profile_version: "local-adoption-sampling-v1",
                temperature: 0.1,
                top_p: 0.8,
            },
            thinking_control: ModelThinkingControl::ChatTemplateEnableThinkingFalse,
            thinking_source: SourceClaim {
                claim: "Qwen3.5 documents instruct/non-thinking mode through enable_thinking=false.",
                source: "https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode",
                checked_at: "2026-07-30",
                status: "confirmed",
            },
        }),
        benchmark: BenchmarkClaim {
            source: "https://huggingface.co/Qwen/Qwen3.5-4B#benchmark-results",
            checked_at: "2026-06-29",
            claim_status: "source-listed-unreproduced",
            harness: "미확정: upstream model card의 공개 점수 조건을 local harness로 아직 고정하지 않음",
            dataset: "미확정: dataset version/license/source를 local benchmark fixture에 아직 고정하지 않음",
            prompt: "미확정: prompt/template과 sampling option을 아직 고정하지 않음",
            scoring: "미확정: local scorer와 published scorer parity를 아직 확인하지 않음",
            hardware_backend: "미확정: GGUF artifact, quantization, llama.cpp version, hardware 조건을 아직 고정하지 않음",
            reproducibility: "공개 점수는 upstream model card source로만 추적하며, GGUF artifact/backend/quantization 조건이 정해지기 전까지 local parity 미검증입니다.",
        },
        install_blockers: QWEN_4B_BLOCKERS,
    },
    ModelManifestEntry {
        id: "gemma-4-e4b",
        display_name: "Gemma 4 E4B IT QAT Q4_0 GGUF",
        status: CandidateStatus::Unverified,
        role: "비교 평가 후보",
        upstream_model: "google/gemma-4-E4B-it-qat-q4_0-unquantized",
        upstream_url: "https://huggingface.co/google/gemma-4-E4B-it-qat-q4_0-unquantized",
        format: "gguf",
        backend: "llama.cpp",
        license: SourceClaim {
            claim: "Hugging Face model card license field is apache-2.0 and Google's current Gemma page publishes Apache License 2.0.",
            source: "https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf, https://ai.google.dev/gemma/apache_2",
            checked_at: "2026-07-11",
            status: "confirmed",
        },
        artifact_provider: Some("google/gemma-4-E4B-it-qat-q4_0-gguf"),
        artifact_url: Some("https://huggingface.co/google/gemma-4-E4B-it-qat-q4_0-gguf/resolve/bb3b92e6f031fa438b409f898dd9f14f499a0cb0/gemma-4-E4B_q4_0-it.gguf"),
        artifact_terms_url: Some("https://huggingface.co/google/gemma-4-E4B-it-qat-q4_0-gguf"),
        artifact_name: Some("gemma-4-E4B_q4_0-it.gguf"),
        quantization: Some("QAT q4_0"),
        sha256: Some("e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d"),
        size_bytes: Some(5_154_939_136),
        vision_projector: Some(ModelArtifactDescriptor {
            provider: "google/gemma-4-E4B-it-qat-q4_0-gguf",
            url: "https://huggingface.co/google/gemma-4-E4B-it-qat-q4_0-gguf/resolve/bb3b92e6f031fa438b409f898dd9f14f499a0cb0/gemma-4-E4B-it-mmproj.gguf",
            terms_url: "https://huggingface.co/google/gemma-4-E4B-it-qat-q4_0-gguf",
            file_name: "gemma-4-E4B-it-mmproj.gguf",
            sha256: "c6398448d84a4836fdedf58f9775979e69ae0cc4dfdf4d697b5597693a555b12",
            size_bytes: 991_551_904,
        }),
        context_length: Some(131_072),
        recommended_ram_gb: None,
        backend_compatibility: Some(SourceClaim {
            claim: "Hugging Face API lists this artifact as GGUF with architecture gemma4 and endpoints_compatible; compatibility still requires host-local promotion evidence.",
            source: "https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf",
            checked_at: "2026-07-06",
            status: "source-listed-unverified",
        }),
        generation_profile: Some(ModelGenerationProfile {
            sampling: ModelSamplingProfile {
                profile_version: "local-adoption-sampling-v1",
                temperature: 0.1,
                top_p: 0.8,
            },
            thinking_control: ModelThinkingControl::ChatTemplateEnableThinkingFalse,
            thinking_source: SourceClaim {
                claim: "Gemma documents thinking control for supported runtimes.",
                source: "https://ai.google.dev/gemma/docs/capabilities/thinking",
                checked_at: "2026-07-30",
                status: "confirmed",
            },
        }),
        benchmark: BenchmarkClaim {
            source: "https://huggingface.co/google/gemma-4-E4B#benchmark-results",
            checked_at: "2026-06-29",
            claim_status: "source-listed-unreproduced",
            harness: "미확정: upstream model card의 공개 점수 조건을 local harness로 아직 고정하지 않음",
            dataset: "미확정: dataset version/license/source를 local benchmark fixture에 아직 고정하지 않음",
            prompt: "미확정: prompt/template과 sampling option을 아직 고정하지 않음",
            scoring: "미확정: local scorer와 published scorer parity를 아직 확인하지 않음",
            hardware_backend: "미확정: GGUF artifact, quantization, llama.cpp version, hardware 조건을 아직 고정하지 않음",
            reproducibility: "공개 점수는 upstream model card source로만 추적하며, GGUF artifact/backend/quantization 조건이 정해지기 전까지 local parity 미검증입니다.",
        },
        install_blockers: GEMMA_4B_BLOCKERS,
    },
    ModelManifestEntry {
        id: "qwen3.5-9b",
        display_name: "Qwen3.5 9B GGUF",
        status: CandidateStatus::Candidate,
        role: "품질 참고 후보",
        upstream_model: "Qwen/Qwen3.5-9B",
        upstream_url: "https://huggingface.co/Qwen/Qwen3.5-9B",
        format: "gguf",
        backend: "llama.cpp",
        license: SourceClaim {
            claim: "Hugging Face model card license field is apache-2.0.",
            source: "https://huggingface.co/Qwen/Qwen3.5-9B",
            checked_at: "2026-06-29",
            status: "confirmed",
        },
        artifact_provider: None,
        artifact_url: None,
        artifact_terms_url: None,
        artifact_name: None,
        quantization: None,
        sha256: None,
        size_bytes: None,
        vision_projector: None,
        context_length: None,
        recommended_ram_gb: None,
        backend_compatibility: None,
        generation_profile: None,
        benchmark: BenchmarkClaim {
            source: "https://huggingface.co/Qwen/Qwen3.5-9B#benchmark-results",
            checked_at: "2026-06-29",
            claim_status: "source-listed-unreproduced",
            harness: "미확정: upstream model card의 공개 점수 조건을 local harness로 아직 고정하지 않음",
            dataset: "미확정: dataset version/license/source를 local benchmark fixture에 아직 고정하지 않음",
            prompt: "미확정: prompt/template과 sampling option을 아직 고정하지 않음",
            scoring: "미확정: local scorer와 published scorer parity를 아직 확인하지 않음",
            hardware_backend: "미확정: GGUF artifact, quantization, llama.cpp version, hardware 조건을 아직 고정하지 않음",
            reproducibility: "공개 점수는 upstream model card source로만 추적하며, 16 GB runtime fit과 local parity는 측정 전 미확정입니다.",
        },
        install_blockers: QWEN_9B_BLOCKERS,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManifestCounts {
    pub(crate) total: usize,
    pub(crate) candidate: usize,
    pub(crate) unverified: usize,
    pub(crate) verified: usize,
}

impl ManifestCounts {
    pub(crate) fn from_candidates() -> Self {
        let mut counts = Self {
            total: CANDIDATES.len(),
            candidate: 0,
            unverified: 0,
            verified: 0,
        };
        for candidate in CANDIDATES {
            match candidate.status {
                CandidateStatus::Candidate => counts.candidate += 1,
                CandidateStatus::Unverified => counts.unverified += 1,
                CandidateStatus::Verified => counts.verified += 1,
            }
        }
        counts
    }
}

#[cfg(test)]
#[path = "manifest/tests.rs"]
mod tests;
