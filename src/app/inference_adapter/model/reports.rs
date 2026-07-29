use crate::adapters::filesystem::model_artifact::{self, promotion_evidence_path};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact_blockers, validate_install_ready, ManifestCounts,
    CANDIDATES, STATUS_SCHEMA,
};

use super::evidence::local_promotion_readiness;
use super::registry::{install_ready_for_report, registry_summary};

pub fn candidate_summary() -> String {
    let counts = ManifestCounts::from_candidates();
    format!(
        "{}개 후보, static verified {}개, static 설치 가능 {}개, local artifact/promotion audit deferred",
        counts.total,
        counts.verified,
        CANDIDATES
            .iter()
            .filter(|candidate| validate_install_ready(candidate).ready)
            .count()
    )
}

pub fn list_report() -> String {
    let counts = ManifestCounts::from_candidates();
    let registry = registry_summary();
    let mut output = format!(
        "모델 manifest\n- schema version: 1\n- 후보: {}개\n- candidate: {}개\n- unverified: {}개\n- verified: {}개\n- 설치 가능: {}개\n- local registry: {}\n\n",
        counts.total,
        counts.candidate,
        counts.unverified,
        counts.verified,
        CANDIDATES
            .iter()
            .filter(|candidate| install_ready_for_report(candidate))
            .count(),
        model_artifact::paths().registry_dir.display()
    );

    for candidate in CANDIDATES {
        let validation = validate_install_ready(candidate);
        let promotion_ready = local_promotion_readiness(candidate)
            .map(|readiness| readiness.validation.ready)
            .unwrap_or(false);
        let install_state = if validation.ready || promotion_ready {
            "설치 가능"
        } else {
            "설치 차단"
        };

        output.push_str(&format!(
            "- {} ({})\n  상태: {} / {}\n  역할: {}\n  upstream model: {}\n  upstream source: {}\n  license claim: {} ({}, checked {})\n  artifact: {}\n  sha256: {}\n  public benchmark source: {} ({})\n  reproducibility: {}\n  parity conditions: harness={}, dataset={}, prompt={}, scoring={}, hardware/backend={}\n",
            candidate.id,
            candidate.display_name,
            candidate.status.label(),
            install_state,
            candidate.role,
            candidate.upstream_model,
            candidate.upstream_url,
            candidate.license.status,
            candidate.license.source,
            candidate.license.checked_at,
            candidate.artifact_url.unwrap_or("미확정"),
            candidate.sha256.unwrap_or("미확정"),
            candidate.benchmark.source,
            candidate.benchmark.claim_status,
            candidate.benchmark.reproducibility,
            candidate.benchmark.harness,
            candidate.benchmark.dataset,
            candidate.benchmark.prompt,
            candidate.benchmark.scoring,
            candidate.benchmark.hardware_backend
        ));
    }

    output.push('\n');
    output.push_str(&registry);
    output.push_str("\n\n설치 가능 상태가 되려면 후보가 verified 상태여야 하고, GGUF URL, provider terms, SHA-256, file size, backend 호환성, RAM 근거가 source-backed manifest에 있어야 합니다.");
    output
}

pub fn manifest_report() -> String {
    format!(
        "model manifest schema\n- schemaVersion: 1\n- required status: {}\n- required source-backed fields: upstreamModel, upstreamUrl, license, licenseSource, licenseCheckedAt, artifactUrl, artifactProvider, artifactTermsUrl, sha256, sizeBytes, quantization, backendCompatibility, recommendedRamEvidence\n- benchmark ledger fields: publishedScoreSource, checkedAt, harness, dataset, scoring, backend, quantization, contextLength, localScore, parityStatus\n- install gate: static verified manifest 또는 verified-local-promotion evidence\n- local promotion gate: artifact checksum/size match, backend smoke ledger event, RAM-fit evidence, mmproj evidence, measured-local benchmark row\n- local evidence: app data models/evidence/<model-id>.promotion.json\n- local registry: app data models/registry/<model-id>.json\n- 금지: checksum 없는 설치, license 미표기 설치, 출처 없는 RAM/backend/benchmark claim 확정",
        STATUS_SCHEMA
            .iter()
            .map(|status| status.label())
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

pub fn inspect_report(id: &str) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let validation = validate_install_ready(candidate);
    let promotion = local_promotion_readiness(candidate)?;
    Ok(format!(
        "model inspect\n- id: {}\n- display name: {}\n- status: {}\n- install ready: {}\n- blockers: {}\n- local promotion ready: {}\n- local promotion evidence: {}\n- local promotion blockers: {}\n- upstream model: {}\n- upstream source: {}\n- license claim: {}\n- license source: {}\n- license checked-at: {}\n- artifact provider: {}\n- artifact URL: {}\n- artifact terms: {}\n- artifact name: {}\n- format: {}\n- backend: {}\n- quantization: {}\n- sha256: {}\n- size bytes: {}\n- context length: {}\n- recommended RAM GB: {}\n- backend compatibility: {}\n- public benchmark source: {}\n- benchmark checked-at: {}\n- benchmark claim status: {}\n- benchmark harness: {}\n- benchmark dataset: {}\n- benchmark prompt: {}\n- benchmark scoring: {}\n- benchmark hardware/backend: {}\n- reproducibility: {}",
        candidate.id,
        candidate.display_name,
        candidate.status.label(),
        if validation.ready || promotion.validation.ready {
            "yes"
        } else {
            "no"
        },
        display_vec(&validation.blockers),
        if promotion.validation.ready { "yes" } else { "no" },
        promotion_evidence_path(candidate.id).display(),
        display_vec(&promotion.validation.blockers),
        candidate.upstream_model,
        candidate.upstream_url,
        candidate.license.claim,
        candidate.license.source,
        candidate.license.checked_at,
        candidate.artifact_provider.unwrap_or("미확정"),
        candidate.artifact_url.unwrap_or("미확정"),
        candidate.artifact_terms_url.unwrap_or("미확정"),
        candidate.artifact_name.unwrap_or("미확정"),
        candidate.format,
        candidate.backend,
        candidate.quantization.unwrap_or("미확정"),
        candidate.sha256.unwrap_or("미확정"),
        candidate
            .size_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        candidate
            .context_length
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        candidate
            .recommended_ram_gb
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        candidate
            .backend_compatibility
            .map(|claim| format!("{} ({}, checked {})", claim.status, claim.source, claim.checked_at))
            .unwrap_or_else(|| "미확정".to_string()),
        candidate.benchmark.source,
        candidate.benchmark.checked_at,
        candidate.benchmark.claim_status,
        candidate.benchmark.harness,
        candidate.benchmark.dataset,
        candidate.benchmark.prompt,
        candidate.benchmark.scoring,
        candidate.benchmark.hardware_backend,
        candidate.benchmark.reproducibility
    ))
}

pub fn download_plan_report(id: &str) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let validation = validate_install_ready(candidate);
    let download_status = if validation.ready { "ready" } else { "blocked" };

    Ok(format!(
        "model download plan\n- id: {}\n- status: {}\n- source: {}\n- license source: {}\n- license checked-at: {}\n- artifact provider: {}\n- artifact URL: {}\n- artifact terms: {}\n- file name: {}\n- size bytes: {}\n- sha256: {}\n- resume path: {}\n- final path: {}\n- blockers: {}\n- 동작: 실제 다운로드 전 위 source/license/checksum/size/provider terms를 사용자에게 표시해야 합니다.",
        candidate.id,
        download_status,
        candidate.upstream_url,
        candidate.license.source,
        candidate.license.checked_at,
        candidate.artifact_provider.unwrap_or("미확정"),
        candidate.artifact_url.unwrap_or("미확정"),
        candidate.artifact_terms_url.unwrap_or("미확정"),
        candidate.artifact_name.unwrap_or("미확정"),
        candidate
            .size_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        candidate.sha256.unwrap_or("미확정"),
        model_artifact::paths().partial(candidate.id).display(),
        model_artifact::paths()
            .artifact(candidate.artifact_name.unwrap_or(candidate.id))
            .display(),
        display_vec(&validation.blockers)
    ))
}

pub fn benchmark_plan_report(id: &str) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let artifact_status = if source_backed_artifact_blockers(candidate).is_empty() {
        "source-backed-artifact-recorded"
    } else {
        "source-backed-artifact-incomplete"
    };
    let public_parity_status = if candidate.benchmark.harness.contains("미확정")
        || candidate.benchmark.dataset.contains("미확정")
        || candidate.benchmark.scoring.contains("미확정")
        || candidate.benchmark.hardware_backend.contains("미확정")
    {
        "blocked-until-conditions-fixed"
    } else {
        "ready-for-local-reproduction"
    };

    Ok(format!(
        "model benchmark plan\n- id: {}\n- manifest status: {}\n- artifact status: {}\n- public benchmark source: {}\n- public benchmark checked-at: {}\n- public benchmark claim status: {}\n- public benchmark parity status: {}\n- required public parity fields:\n  - harness: {}\n  - dataset: {}\n  - prompt/template: {}\n  - scoring: {}\n  - hardware/backend: {}\n- local product benchmark suite:\n  - final Korean response stability\n  - repository exploration accuracy\n  - small patch generation and diff applicability\n  - verification output interpretation\n  - safe stop / command policy compliance\n- runtime metrics to capture: first token latency, tokens/sec, peak memory, prompt/completion/context tokens, context drops, ontology/tool-summary tokens, backend startup time\n- scoring gate: average >= 2.2, Korean failure <= 5%, invalid diff <= 10%, destructive policy violations = 0\n- published-vs-local rule: do not compare scores as equal until artifact, quantization, backend, context length, prompt/template, dataset version, and scoring method are recorded together\n- next: run `rpotato model eval-plan {}` first, then execute local smoke/benchmark only after the artifact and backend state are ready.",
        candidate.id,
        candidate.status.label(),
        artifact_status,
        candidate.benchmark.source,
        candidate.benchmark.checked_at,
        candidate.benchmark.claim_status,
        public_parity_status,
        candidate.benchmark.harness,
        candidate.benchmark.dataset,
        candidate.benchmark.prompt,
        candidate.benchmark.scoring,
        candidate.benchmark.hardware_backend,
        candidate.id
    ))
}

fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}
