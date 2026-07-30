#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateStatus {
    Candidate,
    Unverified,
    Verified,
}

impl CandidateStatus {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Unverified => "unverified",
            Self::Verified => "verified",
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModelArtifactDescriptor {
    pub(crate) provider: &'static str,
    pub(crate) url: &'static str,
    pub(crate) terms_url: &'static str,
    pub(crate) file_name: &'static str,
    pub(crate) sha256: &'static str,
    pub(crate) size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ModelSamplingProfile {
    pub(crate) profile_version: &'static str,
    pub(crate) temperature: f64,
    pub(crate) top_p: f64,
}

impl ModelSamplingProfile {
    pub(crate) fn ledger_label(self) -> String {
        format!("temperature-{}_top-p-{}", self.temperature, self.top_p)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelThinkingControl {
    ChatTemplateEnableThinkingFalse,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModelGenerationProfile {
    pub(crate) sampling: ModelSamplingProfile,
    pub(crate) thinking_control: ModelThinkingControl,
    pub(crate) thinking_source: SourceClaim,
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
    pub(crate) vision_projector: Option<ModelArtifactDescriptor>,
    pub(crate) context_length: Option<u32>,
    pub(crate) recommended_ram_gb: Option<u32>,
    pub(crate) backend_compatibility: Option<SourceClaim>,
    pub(crate) generation_profile: Option<ModelGenerationProfile>,
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
    pub(crate) vision_status: String,
    pub(crate) mmproj_path: Option<String>,
    pub(crate) mmproj_sha256: Option<String>,
    pub(crate) mmproj_size_bytes: Option<u64>,
    pub(crate) license_source: String,
    pub(crate) license_checked_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegistryVisionState {
    pub(crate) status: String,
    pub(crate) mmproj_path: Option<String>,
    pub(crate) mmproj_sha256: Option<String>,
    pub(crate) mmproj_size_bytes: Option<u64>,
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
