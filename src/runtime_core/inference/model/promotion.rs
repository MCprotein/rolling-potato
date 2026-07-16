use std::path::Path;

use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::benchmark as benchmark_policy;

use super::manifest::{
    BackendSmokeEvidence, InstallValidation, LocalArtifactState, ModelArtifactDescriptor,
    ModelManifestEntry, PromotionEvidence,
};

pub(crate) const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PromotionBenchmarkEvidence {
    pub(crate) claim_state: String,
    pub(crate) local_pass: Option<bool>,
    pub(crate) backend_id: Option<String>,
    pub(crate) fixture_id: String,
    pub(crate) fixture_sha256: String,
    pub(crate) prompt_artifact_sha256: Option<String>,
    pub(crate) benchmark_name: String,
    pub(crate) dataset_ref: Option<String>,
    pub(crate) peak_rss_bytes: Option<u64>,
    pub(crate) model_run_id: Option<String>,
}

pub(crate) fn validate_promotion_evidence(
    candidate: &ModelManifestEntry,
    evidence: &PromotionEvidence,
    artifact: ModelArtifactDescriptor,
    local_state: &LocalArtifactState,
    benchmark: Option<&PromotionBenchmarkEvidence>,
    backend_smoke: Option<&BackendSmokeEvidence>,
) -> InstallValidation {
    let mut blockers = Vec::new();

    if evidence.model_id != candidate.id {
        push_unique(
            &mut blockers,
            &format!(
                "evidence modelId가 후보와 다릅니다: expected {}, actual {}",
                candidate.id, evidence.model_id
            ),
        );
    }
    if evidence.artifact_sha256 != artifact.sha256 {
        push_unique(
            &mut blockers,
            "evidence artifactSha256이 source-backed manifest와 일치하지 않습니다.",
        );
    }
    if evidence.artifact_size_bytes != artifact.size_bytes {
        push_unique(
            &mut blockers,
            "evidence artifactSizeBytes가 source-backed manifest와 일치하지 않습니다.",
        );
    }
    if evidence.backend_id != candidate.backend {
        push_unique(
            &mut blockers,
            "evidence backendId가 후보 backend와 일치하지 않습니다.",
        );
    }
    if evidence.backend_version.trim().is_empty() {
        push_unique(&mut blockers, "backendVersion evidence가 비어 있습니다.");
    }
    match backend_smoke {
        Some(smoke) => {
            if smoke.backend_id != candidate.backend {
                push_unique(&mut blockers, "backend smoke backend가 후보와 다릅니다.");
            }
            if smoke.backend_release != evidence.backend_version {
                push_unique(
                    &mut blockers,
                    "backend smoke release가 promotion evidence와 다릅니다.",
                );
            }
            if !checksum::is_valid_sha256(&smoke.binary_sha256) {
                push_unique(
                    &mut blockers,
                    "backend smoke binary SHA-256이 유효하지 않습니다.",
                );
            }
            if smoke.model_id != artifact_model_id(artifact)
                || smoke.model_sha256 != artifact.sha256
                || smoke.model_size_bytes != artifact.size_bytes
            {
                push_unique(
                    &mut blockers,
                    "backend smoke model artifact provenance가 후보 manifest와 다릅니다.",
                );
            }
            if smoke.ctx_size == "model-default" || smoke.ctx_size.parse::<u32>().is_err() {
                push_unique(
                    &mut blockers,
                    "backend smoke context size가 고정되지 않았습니다.",
                );
            }
            if smoke.mmproj != evidence.mmproj {
                push_unique(
                    &mut blockers,
                    "backend smoke mmproj 결과가 evidence와 다릅니다.",
                );
            }
            if smoke.sampling != "temperature-0.1_top-p-0.8" {
                push_unique(
                    &mut blockers,
                    "backend smoke sampling 조건이 고정값과 다릅니다.",
                );
            }
            if smoke.host_os.trim().is_empty() || smoke.host_arch.trim().is_empty() {
                push_unique(
                    &mut blockers,
                    "backend smoke host 환경 evidence가 비어 있습니다.",
                );
            }
        }
        None => push_unique(
            &mut blockers,
            "동일 artifact provenance를 가진 backend chat smoke event를 확인하지 못했습니다.",
        ),
    }
    if !local_state.verified {
        push_unique(
            &mut blockers,
            &format!(
                "local artifact가 manifest와 일치하지 않습니다: {}",
                local_state.detail
            ),
        );
    }
    if evidence.ram_fit != "observed-within-local-host" {
        push_unique(
            &mut blockers,
            "ramFit은 observed-within-local-host여야 합니다.",
        );
    }
    if evidence.recommended_ram_gb == 0 {
        push_unique(&mut blockers, "recommendedRamGb는 1 이상이어야 합니다.");
    }
    if evidence.peak_rss_bytes == 0 {
        push_unique(&mut blockers, "peakRssBytes RAM evidence가 필요합니다.");
    }
    let ram_budget_bytes = (evidence.recommended_ram_gb as u64).saturating_mul(BYTES_PER_GIB);
    if evidence.peak_rss_bytes > ram_budget_bytes {
        push_unique(
            &mut blockers,
            "peakRssBytes가 recommendedRamGb budget을 초과합니다.",
        );
    }
    if evidence.recommended_ram_gb != measured_ram_budget_gb(evidence.peak_rss_bytes) {
        push_unique(
            &mut blockers,
            "recommendedRamGb는 measured peak RSS + 2 GiB headroom 공식과 일치해야 합니다.",
        );
    }
    if !matches!(
        evidence.mmproj.as_str(),
        "not-required-text-only" | "not-required" | "required"
    ) {
        push_unique(
            &mut blockers,
            "mmproj evidence는 not-required-text-only, not-required, required 중 하나여야 합니다.",
        );
    }

    match benchmark {
        Some(row) => {
            if row.claim_state != "measured-locally" {
                push_unique(
                    &mut blockers,
                    "benchmark claim_state는 measured-locally여야 합니다.",
                );
            }
            if row.local_pass != Some(true) {
                push_unique(
                    &mut blockers,
                    "benchmark local_pass=true evidence가 필요합니다.",
                );
            }
            if row.backend_id.as_deref() != Some(candidate.backend) {
                push_unique(
                    &mut blockers,
                    "benchmark backend_id가 후보 backend와 일치하지 않습니다.",
                );
            }
            if row.fixture_id != benchmark_policy::ADOPTION_FIXTURE_ID
                || row.fixture_sha256 != benchmark_policy::ADOPTION_FIXTURE_SHA256
                || row.prompt_artifact_sha256.as_deref()
                    != Some(benchmark_policy::ADOPTION_PROMPT_SHA256)
                || row.benchmark_name != benchmark_policy::ADOPTION_BENCHMARK_NAME
                || row.dataset_ref.as_deref() != Some(benchmark_policy::ADOPTION_DATASET_REF)
            {
                push_unique(
                    &mut blockers,
                    "benchmark가 canonical model adoption smoke fixture가 아닙니다.",
                );
            }
            if row.peak_rss_bytes != Some(evidence.peak_rss_bytes) {
                push_unique(
                    &mut blockers,
                    "benchmark peak_rss_bytes가 promotion evidence와 일치하지 않습니다.",
                );
            }
            if row.model_run_id.as_deref()
                != Some(format!("model-run-{}", evidence.backend_smoke_event_id).as_str())
            {
                push_unique(
                    &mut blockers,
                    "benchmark model_run_id가 backend smoke event와 직접 연결되지 않았습니다.",
                );
            }
        }
        None => push_unique(
            &mut blockers,
            "benchmarkRunId에 대응하는 measured local benchmark evidence가 없습니다.",
        ),
    }

    InstallValidation {
        ready: blockers.is_empty(),
        blockers,
    }
}

pub(crate) fn measured_ram_budget_gb(peak_rss_bytes: u64) -> u32 {
    let measured_gib = peak_rss_bytes.saturating_add(BYTES_PER_GIB - 1) / BYTES_PER_GIB;
    measured_gib.saturating_add(2).min(u64::from(u32::MAX)) as u32
}

pub(crate) fn artifact_model_id(artifact: ModelArtifactDescriptor) -> String {
    Path::new(artifact.file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(artifact.file_name)
        .to_string()
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}
