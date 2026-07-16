#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateStatus {
    Candidate,
    Unverified,
    Verified,
}

pub(crate) const STATUS_SCHEMA: &[CandidateStatus] = &[
    CandidateStatus::Candidate,
    CandidateStatus::Unverified,
    CandidateStatus::Verified,
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct SourceClaim {
    pub(crate) claim: &'static str,
    pub(crate) source: &'static str,
    pub(crate) checked_at: &'static str,
    pub(crate) status: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BenchmarkClaim {
    pub(crate) source: &'static str,
    pub(crate) checked_at: &'static str,
    pub(crate) claim_status: &'static str,
    pub(crate) harness: &'static str,
    pub(crate) dataset: &'static str,
    pub(crate) prompt: &'static str,
    pub(crate) scoring: &'static str,
    pub(crate) hardware_backend: &'static str,
    pub(crate) reproducibility: &'static str,
}

#[derive(Debug)]
pub(crate) struct ModelManifestEntry {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) status: CandidateStatus,
    pub(crate) role: &'static str,
    pub(crate) upstream_model: &'static str,
    pub(crate) upstream_url: &'static str,
    pub(crate) format: &'static str,
    pub(crate) backend: &'static str,
    pub(crate) license: SourceClaim,
    pub(crate) artifact_provider: Option<&'static str>,
    pub(crate) artifact_url: Option<&'static str>,
    pub(crate) artifact_terms_url: Option<&'static str>,
    pub(crate) artifact_name: Option<&'static str>,
    pub(crate) quantization: Option<&'static str>,
    pub(crate) sha256: Option<&'static str>,
    pub(crate) size_bytes: Option<u64>,
    pub(crate) context_length: Option<u32>,
    pub(crate) recommended_ram_gb: Option<u32>,
    pub(crate) backend_compatibility: Option<SourceClaim>,
    pub(crate) benchmark: BenchmarkClaim,
    pub(crate) install_blockers: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallValidation {
    pub(crate) ready: bool,
    pub(crate) blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegistryEntry {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) status: String,
    pub(crate) evidence_status: String,
    pub(crate) promotion_evidence_path: String,
    pub(crate) backend_version: String,
    pub(crate) benchmark_run_id: String,
    pub(crate) upstream_model: String,
    pub(crate) upstream_url: String,
    pub(crate) artifact_path: String,
    pub(crate) artifact_sha256: String,
    pub(crate) license_source: String,
    pub(crate) license_checked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefaultSelection {
    pub(crate) model_id: String,
    pub(crate) artifact_sha256: String,
    pub(crate) selected_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelArtifactFetchStatus {
    Downloaded,
    Resumed,
    CacheHit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModelArtifactDescriptor {
    pub(crate) provider: &'static str,
    pub(crate) url: &'static str,
    pub(crate) terms_url: &'static str,
    pub(crate) file_name: &'static str,
    pub(crate) sha256: &'static str,
    pub(crate) size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalArtifactState {
    pub(crate) status: &'static str,
    pub(crate) detail: String,
    pub(crate) verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromotionEvidence {
    pub(crate) model_id: String,
    pub(crate) artifact_sha256: String,
    pub(crate) artifact_size_bytes: u64,
    pub(crate) backend_id: String,
    pub(crate) backend_version: String,
    pub(crate) backend_smoke_event_id: String,
    pub(crate) ram_fit: String,
    pub(crate) recommended_ram_gb: u32,
    pub(crate) peak_rss_bytes: u64,
    pub(crate) mmproj: String,
    pub(crate) benchmark_run_id: String,
    pub(crate) recorded_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PromotionReadiness {
    pub(crate) validation: InstallValidation,
    pub(crate) evidence: Option<PromotionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendSmokeEvidence {
    pub(crate) event_id: String,
    pub(crate) backend_id: String,
    pub(crate) backend_release: String,
    pub(crate) binary_sha256: String,
    pub(crate) model_id: String,
    pub(crate) model_sha256: String,
    pub(crate) model_size_bytes: u64,
    pub(crate) ctx_size: String,
    pub(crate) mmproj: String,
    pub(crate) sampling: String,
    pub(crate) host_os: String,
    pub(crate) host_arch: String,
}

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
        context_length: Some(262_144),
        recommended_ram_gb: None,
        backend_compatibility: Some(SourceClaim {
            claim: "Hugging Face API lists this artifact as GGUF with architecture qwen35 and endpoints_compatible; compatibility still requires host-local promotion evidence.",
            source: "https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF",
            checked_at: "2026-07-06",
            status: "source-listed-unverified",
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
        context_length: Some(131_072),
        recommended_ram_gb: None,
        backend_compatibility: Some(SourceClaim {
            claim: "Hugging Face API lists this artifact as GGUF with architecture gemma4 and endpoints_compatible; compatibility still requires host-local promotion evidence.",
            source: "https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf",
            checked_at: "2026-07-06",
            status: "source-listed-unverified",
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
        context_length: None,
        recommended_ram_gb: None,
        backend_compatibility: None,
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
