use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::app::AppError;
use crate::{checksum, ledger, observability, paths, state};

const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;
const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateStatus {
    Candidate,
    Unverified,
    Verified,
}

const STATUS_SCHEMA: &[CandidateStatus] = &[
    CandidateStatus::Candidate,
    CandidateStatus::Unverified,
    CandidateStatus::Verified,
];

#[derive(Debug, Clone, Copy)]
struct SourceClaim {
    claim: &'static str,
    source: &'static str,
    checked_at: &'static str,
    status: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct BenchmarkClaim {
    source: &'static str,
    checked_at: &'static str,
    claim_status: &'static str,
    harness: &'static str,
    dataset: &'static str,
    prompt: &'static str,
    scoring: &'static str,
    hardware_backend: &'static str,
    reproducibility: &'static str,
}

#[derive(Debug)]
struct ModelManifestEntry {
    id: &'static str,
    display_name: &'static str,
    status: CandidateStatus,
    role: &'static str,
    upstream_model: &'static str,
    upstream_url: &'static str,
    format: &'static str,
    backend: &'static str,
    license: SourceClaim,
    artifact_provider: Option<&'static str>,
    artifact_url: Option<&'static str>,
    artifact_terms_url: Option<&'static str>,
    artifact_name: Option<&'static str>,
    quantization: Option<&'static str>,
    sha256: Option<&'static str>,
    size_bytes: Option<u64>,
    context_length: Option<u32>,
    recommended_ram_gb: Option<u32>,
    backend_compatibility: Option<SourceClaim>,
    benchmark: BenchmarkClaim,
    install_blockers: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallValidation {
    ready: bool,
    blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryEntry {
    id: String,
    display_name: String,
    status: String,
    artifact_path: String,
    artifact_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelArtifactFetchStatus {
    Downloaded,
    Resumed,
    CacheHit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ModelArtifactDescriptor {
    provider: &'static str,
    url: &'static str,
    terms_url: &'static str,
    file_name: &'static str,
    sha256: &'static str,
    size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalArtifactState {
    status: &'static str,
    detail: String,
    verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromotionEvidence {
    model_id: String,
    artifact_sha256: String,
    artifact_size_bytes: u64,
    backend_id: String,
    backend_version: String,
    backend_smoke_event_id: String,
    ram_fit: String,
    recommended_ram_gb: u32,
    peak_rss_bytes: u64,
    mmproj: String,
    benchmark_run_id: String,
    recorded_at: String,
}

#[derive(Debug, Clone)]
struct PromotionReadiness {
    validation: InstallValidation,
    evidence: Option<PromotionEvidence>,
}

const QWEN_4B_BLOCKERS: &[&str] = &[
    "local llama.cpp b9878 smoke 미실행",
    "16 GB runtime fit 미측정",
    "텍스트 전용 실행 시 mmproj 필요 여부 미확정",
];
const GEMMA_4B_BLOCKERS: &[&str] = &[
    "local llama.cpp b9878 smoke 미실행",
    "16 GB runtime fit 미측정",
    "텍스트 전용 실행 시 mmproj 필요 여부 미확정",
];
const QWEN_9B_BLOCKERS: &[&str] = &["제품 기본값 보류", "16 GB runtime fit 미측정"];

const CANDIDATES: &[ModelManifestEntry] = &[
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
            claim: "Hugging Face API lists this artifact as GGUF with architecture qwen35 and endpoints_compatible; local llama.cpp b9878 smoke is not yet run.",
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
            claim: "Hugging Face model card license field is apache-2.0 and license_link points to the Gemma 4 license page.",
            source: "https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf, https://ai.google.dev/gemma/docs/gemma_4_license",
            checked_at: "2026-07-06",
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
            claim: "Hugging Face API lists this artifact as GGUF with architecture gemma4 and endpoints_compatible; local llama.cpp b9878 smoke is not yet run.",
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

pub fn candidate_summary() -> String {
    let counts = ManifestCounts::from_candidates();
    format!(
        "{}개 후보, verified {}개, 설치 가능 {}개, artifact 검증 필요",
        counts.total,
        counts.verified,
        CANDIDATES
            .iter()
            .filter(|candidate| install_ready_for_report(candidate))
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
        paths::model_registry_dir().display()
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

pub fn registry_report() -> String {
    registry_summary()
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
        paths::downloads_dir()
            .join(format!("{}.part", candidate.id))
            .display(),
        paths::models_dir()
            .join(candidate.artifact_name.unwrap_or(candidate.id))
            .display(),
        display_vec(&validation.blockers)
    ))
}

pub fn eval_plan_report(id: &str) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let source_blockers = source_backed_artifact_blockers(candidate);
    if !source_blockers.is_empty() {
        return Ok(format!(
            "model evaluation plan\n- id: {}\n- status: blocked-before-artifact-fetch\n- source-backed artifact: missing\n- blockers: {}\n- upstream source: {}\n- license source: {}\n- public benchmark source: {}\n- next: source-backed artifact URL, provider terms, file size, and SHA-256 must be recorded before local smoke or benchmark.",
            candidate.id,
            source_blockers.join(", "),
            candidate.upstream_url,
            candidate.license.source,
            candidate.benchmark.source
        ));
    }

    let artifact = source_backed_artifact(candidate)?;
    let final_path = model_artifact_path(artifact);
    let part_path = model_artifact_part_path(candidate);
    let local_state = local_artifact_state(artifact, &final_path)?;
    let benchmark_status = local_benchmark_status(artifact)?;
    let has_local_measurement = benchmark_status.starts_with("measured-locally");
    let plan_status = if has_local_measurement {
        "local-smoke-measured"
    } else if local_state.verified {
        "ready-for-backend-smoke"
    } else {
        "blocked-before-backend-smoke"
    };
    let next = if has_local_measurement {
        format!(
            "prepare a local promotion evidence JSON, then run `rpotato model promote {} --evidence <file>` before `rpotato model install {}`.",
            candidate.id, candidate.id
        )
    } else if local_state.verified {
        format!(
            "run `rpotato backend install-plan`, verify backend state with `rpotato backend doctor`, then run `rpotato backend start --model {} --ctx-size 4096` for local smoke before benchmark scoring.",
            final_path.display()
        )
    } else {
        format!(
            "run `rpotato model fetch-candidate {} --for-evaluation` only when intentionally downloading the multi-GB artifact.",
            candidate.id
        )
    };

    Ok(format!(
        "model evaluation plan\n- id: {}\n- status: {}\n- manifest status: {}\n- role: {}\n- artifact provider: {}\n- artifact source: {}\n- artifact terms: {}\n- expected file: {}\n- expected size bytes: {}\n- expected sha256: {}\n- local artifact status: {}\n- local artifact detail: {}\n- partial path: {}\n- final path: {}\n- registry: not installed by eval-plan\n- public benchmark source: {}\n- benchmark claim status: {}\n- local benchmark status: {}\n- next: {}",
        candidate.id,
        plan_status,
        candidate.status.label(),
        candidate.role,
        artifact.provider,
        artifact.url,
        artifact.terms_url,
        artifact.file_name,
        artifact.size_bytes,
        artifact.sha256,
        local_state.status,
        local_state.detail,
        part_path.display(),
        final_path.display(),
        candidate.benchmark.source,
        candidate.benchmark.claim_status,
        benchmark_status,
        next
    ))
}

fn local_benchmark_status(artifact: ModelArtifactDescriptor) -> Result<String, AppError> {
    if !paths::observability_db_file().exists() {
        return Ok("not-run".to_string());
    }

    let expected_model_id = artifact_model_id(artifact);
    let latest = observability::benchmark_run_reports()?
        .into_iter()
        .rfind(|row| row.model_id == expected_model_id && row.claim_state == "measured-locally");

    let Some(row) = latest else {
        return Ok("not-run".to_string());
    };

    Ok(format!(
        "measured-locally latest_run={} score={} local_pass={} latency_ms={} total_tokens={} resource_pressure={}",
        row.benchmark_run_id,
        row.score
            .map(|score| format!("{score:.6}"))
            .unwrap_or_else(|| "none".to_string()),
        row.local_pass
            .map(|value| if value { "true" } else { "false" })
            .unwrap_or("unknown"),
        row.latency_ms
            .map(|value| format!("{value:.3}"))
            .unwrap_or_else(|| "unknown".to_string()),
        row.total_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        row.resource_pressure.as_deref().unwrap_or("unknown")
    ))
}

fn local_promotion_readiness(
    candidate: &'static ModelManifestEntry,
) -> Result<PromotionReadiness, AppError> {
    let mut blockers = Vec::new();
    let source_blockers = source_backed_artifact_blockers(candidate);
    if !source_blockers.is_empty() {
        blockers.extend(
            source_blockers
                .into_iter()
                .map(|blocker| format!("source-backed artifact: {blocker}")),
        );
        return Ok(PromotionReadiness {
            validation: InstallValidation {
                ready: false,
                blockers,
            },
            evidence: None,
        });
    }

    let evidence_path = promotion_evidence_path(candidate.id);
    if !evidence_path.exists() {
        blockers.push(format!(
            "local promotion evidence 없음: {}",
            evidence_path.display()
        ));
        return Ok(PromotionReadiness {
            validation: InstallValidation {
                ready: false,
                blockers,
            },
            evidence: None,
        });
    }
    if !evidence_path.is_file() {
        return Err(AppError::blocked(format!(
            "local promotion evidence path가 file이 아닙니다: {}",
            evidence_path.display()
        )));
    }

    let evidence = read_promotion_evidence_file(&evidence_path)?;
    let artifact = source_backed_artifact(candidate)?;
    let final_path = model_artifact_path(artifact);
    let local_state = local_artifact_state(artifact, &final_path)?;
    let benchmark = promotion_benchmark_run(&evidence, artifact)?;
    let backend_smoke_event_seen = backend_smoke_event_seen(&evidence.backend_smoke_event_id)?;
    let validation = validate_promotion_evidence(
        candidate,
        &evidence,
        artifact,
        &local_state,
        benchmark.as_ref(),
        backend_smoke_event_seen,
    );

    Ok(PromotionReadiness {
        validation,
        evidence: Some(evidence),
    })
}

fn promotion_benchmark_run(
    evidence: &PromotionEvidence,
    artifact: ModelArtifactDescriptor,
) -> Result<Option<observability::BenchmarkRunReport>, AppError> {
    if !paths::observability_db_file().exists() {
        return Ok(None);
    }

    let expected_model_id = artifact_model_id(artifact);
    Ok(observability::benchmark_run_reports()?
        .into_iter()
        .rev()
        .find(|row| {
            row.benchmark_run_id == evidence.benchmark_run_id && row.model_id == expected_model_id
        }))
}

fn backend_smoke_event_seen(event_id: &str) -> Result<bool, AppError> {
    if event_id.trim().is_empty() {
        return Ok(false);
    }

    Ok(ledger::read_runtime_events()?.into_iter().any(|event| {
        event.event_id == event_id
            && matches!(
                event.event_type.as_str(),
                "backend.sidecar.start.completed" | "backend.chat.completed"
            )
    }))
}

fn validate_promotion_evidence(
    candidate: &ModelManifestEntry,
    evidence: &PromotionEvidence,
    artifact: ModelArtifactDescriptor,
    local_state: &LocalArtifactState,
    benchmark: Option<&observability::BenchmarkRunReport>,
    backend_smoke_event_seen: bool,
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
    if !backend_smoke_event_seen {
        push_unique(
            &mut blockers,
            "backend smoke ledger event를 확인하지 못했습니다.",
        );
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
    if evidence.ram_fit != "passed" {
        push_unique(&mut blockers, "ramFit은 passed여야 합니다.");
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
            if row.peak_rss_bytes != Some(evidence.peak_rss_bytes) {
                push_unique(
                    &mut blockers,
                    "benchmark peak_rss_bytes가 promotion evidence와 일치하지 않습니다.",
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

fn artifact_model_id(artifact: ModelArtifactDescriptor) -> String {
    Path::new(artifact.file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(artifact.file_name)
        .to_string()
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

pub fn fetch_candidate_for_evaluation_report(id: &str) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let artifact = source_backed_artifact(candidate)?;
    let final_path = model_artifact_path(artifact);
    let part_path = model_artifact_part_path(candidate);
    let fetch_status = fetch_evaluation_artifact(artifact, &final_path, &part_path)?;
    let event_id = state::record_event(
        "model.evaluation_artifact.fetched",
        "검증용 model artifact fetch 완료",
        &format!(
            "model_id={} provider={} artifact={} sha256={} size_bytes={} status={} registry=not_registered",
            candidate.id,
            artifact.provider,
            final_path.display(),
            artifact.sha256,
            artifact.size_bytes,
            fetch_status.label()
        ),
    )?;

    Ok(format!(
        "검증용 model artifact 준비 완료\n- id: {}\n- status: {}\n- provider: {}\n- source: {}\n- terms: {}\n- file: {}\n- size bytes: {}\n- sha256: {}\n- partial path: {}\n- final path: {}\n- registry: not registered\n- ledger event: {}\n- 다음 단계: rpotato backend start --model {} --ctx-size 4096 으로 local smoke를 실행하고, benchmark/RAM-fit/mmproj evidence가 쌓인 뒤에만 verified 승격을 검토합니다.",
        candidate.id,
        fetch_status.label(),
        artifact.provider,
        artifact.url,
        artifact.terms_url,
        artifact.file_name,
        artifact.size_bytes,
        artifact.sha256,
        part_path.display(),
        final_path.display(),
        event_id,
        final_path.display()
    ))
}

pub fn promote_candidate_report(id: &str, evidence_path: &str) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let evidence_source = PathBuf::from(evidence_path);
    let evidence = read_promotion_evidence_file(&evidence_source)?;
    let artifact = source_backed_artifact(candidate)?;
    let final_path = model_artifact_path(artifact);
    let local_state = local_artifact_state(artifact, &final_path)?;
    let benchmark = promotion_benchmark_run(&evidence, artifact)?;
    let backend_smoke_event_seen = backend_smoke_event_seen(&evidence.backend_smoke_event_id)?;
    let validation = validate_promotion_evidence(
        candidate,
        &evidence,
        artifact,
        &local_state,
        benchmark.as_ref(),
        backend_smoke_event_seen,
    );

    if !validation.ready {
        let event_id = state::record_event(
            "model.promotion.blocked",
            "model local promotion evidence 차단",
            &format!(
                "model_id={} evidence={} blockers={}",
                candidate.id,
                evidence_source.display(),
                validation.blockers.join(",")
            ),
        )?;
        return Err(AppError::blocked(format!(
            "model verified 승격을 차단했습니다\n- id: {}\n- evidence: {}\n- blockers:\n- {}\n- local artifact: {}\n- local benchmark: {}\n- ledger event: {}\n- 다음 단계: artifact checksum/size, backend smoke ledger event, RAM-fit/mmproj 판단, measured-local benchmark를 모두 채운 뒤 다시 실행하세요.",
            candidate.id,
            evidence_source.display(),
            validation.blockers.join("\n- "),
            local_state.status,
            benchmark
                .as_ref()
                .map(|row| row.benchmark_run_id.as_str())
                .unwrap_or("missing"),
            event_id
        )));
    }

    let benchmark = benchmark.expect("validated benchmark evidence");
    persist_promotion_evidence(candidate, &evidence, artifact, &benchmark, &evidence_source)?;
    let event_id = state::record_event(
        "model.promotion.verified",
        "model local promotion evidence 검증 완료",
        &format!(
            "model_id={} artifact={} sha256={} benchmark_run_id={} backend_smoke_event_id={} recommended_ram_gb={} peak_rss_bytes={} mmproj={}",
            candidate.id,
            final_path.display(),
            evidence.artifact_sha256,
            evidence.benchmark_run_id,
            evidence.backend_smoke_event_id,
            evidence.recommended_ram_gb,
            evidence.peak_rss_bytes,
            evidence.mmproj
        ),
    )?;

    Ok(format!(
        "model local promotion evidence 검증 완료\n- id: {}\n- status: verified-local-promotion\n- evidence source: {}\n- normalized evidence: {}\n- artifact: {}\n- artifact sha256: {}\n- backend: {} {}\n- backend smoke event: {}\n- benchmark run: {}\n- recommended RAM GB: {}\n- peak RSS bytes: {}\n- mmproj: {}\n- ledger event: {}\n- 다음 단계: rpotato model install {} 로 registry 등록을 진행할 수 있습니다.",
        candidate.id,
        evidence_source.display(),
        promotion_evidence_path(candidate.id).display(),
        final_path.display(),
        artifact.sha256,
        evidence.backend_id,
        evidence.backend_version,
        evidence.backend_smoke_event_id,
        benchmark.benchmark_run_id,
        evidence.recommended_ram_gb,
        evidence.peak_rss_bytes,
        evidence.mmproj,
        event_id,
        candidate.id
    ))
}

pub fn verify_file_report(path: &str, expected_sha256: &str) -> Result<String, AppError> {
    if !checksum::is_valid_sha256(expected_sha256) {
        return Err(AppError::usage(
            "expected SHA-256은 64자리 hex string이어야 합니다.",
        ));
    }

    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(AppError::usage(format!(
            "검증 대상 파일을 찾지 못했습니다: {}",
            path.display()
        )));
    }

    let actual_sha256 = checksum::sha256_file(&path)?;
    let matched = actual_sha256.eq_ignore_ascii_case(expected_sha256);
    let event_type = if matched {
        "model.sha256.verified"
    } else {
        "model.sha256.rejected"
    };
    let summary = if matched {
        "model artifact SHA-256 검증 성공"
    } else {
        "model artifact SHA-256 검증 실패"
    };
    let event_id = state::record_event(
        event_type,
        summary,
        &format!(
            "path={} expected_sha256={} actual_sha256={}",
            path.display(),
            expected_sha256,
            actual_sha256
        ),
    )?;

    if !matched {
        return Err(AppError::blocked(format!(
            "SHA-256 검증 실패\n- path: {}\n- expected: {}\n- actual: {}\n- ledger event: {}\n- 동작: registry 등록을 차단해야 하며, 실패 artifact 정리는 별도 cleanup phase에서 처리합니다.",
            path.display(),
            expected_sha256,
            actual_sha256,
            event_id
        )));
    }

    Ok(format!(
        "SHA-256 검증 성공\n- path: {}\n- expected: {}\n- actual: {}\n- ledger event: {}",
        path.display(),
        expected_sha256,
        actual_sha256,
        event_id
    ))
}

pub fn cleanup_failed_report(id: &str, dry_run: bool) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let cleanup_paths = failed_artifact_paths(candidate);
    let mut rows = Vec::new();
    let mut removed = 0;
    let mut missing = 0;

    for path in cleanup_paths {
        if !path.exists() {
            missing += 1;
            rows.push(format!("- {} | missing", path.display()));
            continue;
        }

        if !path.is_file() {
            return Err(AppError::blocked(format!(
                "failed artifact cleanup 대상은 file이어야 합니다: {}",
                path.display()
            )));
        }

        if dry_run {
            rows.push(format!("- {} | would delete", path.display()));
        } else {
            fs::remove_file(&path).map_err(|err| {
                AppError::runtime(format!(
                    "failed artifact를 삭제하지 못했습니다: {} ({err})",
                    path.display()
                ))
            })?;
            removed += 1;
            rows.push(format!("- {} | deleted", path.display()));
        }
    }

    let event_id = state::record_event(
        if dry_run {
            "model.failed_artifact.cleanup.planned"
        } else {
            "model.failed_artifact.cleanup.completed"
        },
        if dry_run {
            "failed model artifact cleanup dry-run"
        } else {
            "failed model artifact cleanup 완료"
        },
        &format!(
            "model_id={} dry_run={} removed={} missing={}",
            candidate.id, dry_run, removed, missing
        ),
    )?;

    Ok(format!(
        "failed artifact cleanup {}\n- id: {}\n- removed: {}\n- missing: {}\n- ledger event: {}\n{}\n- boundary: app data downloads/models 아래의 failed/partial artifact만 대상으로 합니다.",
        if dry_run { "dry-run" } else { "결과" },
        candidate.id,
        removed,
        missing,
        event_id,
        rows.join("\n")
    ))
}

pub fn install_candidate(id: &str) -> Result<(), AppError> {
    let candidate = find_candidate(id)?;
    let manifest_validation = validate_install_ready(candidate);
    let promotion = local_promotion_readiness(candidate)?;
    let promotion_ready = promotion.validation.ready;

    if !manifest_validation.ready && !promotion_ready {
        let mut blockers = manifest_validation
            .blockers
            .iter()
            .map(|blocker| format!("manifest: {blocker}"))
            .collect::<Vec<_>>();
        blockers.extend(
            promotion
                .validation
                .blockers
                .iter()
                .map(|blocker| format!("local promotion: {blocker}")),
        );
        let event_id = state::record_event(
            "model.install.blocked",
            "미검증 model install 차단",
            &format!(
                "model_id={} status={} blockers={}",
                candidate.id,
                candidate.status.label(),
                blockers.join(",")
            ),
        )?;
        return Err(AppError::blocked(format!(
            "설치를 차단했습니다: {}\n상태: {}\n이유:\n- {}\nsource: {}\nlicense source: {}\nbenchmark source: {}\nlocal registry: {}\nledger event: {}\n다음 단계: source-recorded artifact field를 유지하면서 local backend smoke, RAM-fit/mmproj 측정, byte-level SHA-256 검증, benchmark evidence를 채운 뒤 verified 상태로 승격해야 합니다.",
            candidate.id,
            candidate.status.label(),
            blockers.join("\n- "),
            candidate.upstream_url,
            candidate.license.source,
            candidate.benchmark.source,
            paths::model_registry_dir().display(),
            event_id
        )));
    }

    let promotion_for_registry = if promotion_ready {
        promotion.evidence.as_ref()
    } else {
        None
    };
    persist_registry_entry(candidate, promotion_for_registry)?;
    let event_id = state::record_event(
        "model.install.registered",
        "검증된 model registry 등록",
        &format!(
            "model_id={} promotion_ready={} evidence={}",
            candidate.id,
            promotion_ready,
            promotion_evidence_path(candidate.id).display()
        ),
    )?;

    println!(
        "모델 registry 등록 완료\n- id: {}\n- registry: {}\n- promotion evidence: {}\n- ledger event: {}\n- 동작: registry 등록 전 artifact checksum/size와 local promotion evidence를 재검증했습니다.",
        candidate.id,
        registry_path(candidate.id).display(),
        if promotion_ready {
            promotion_evidence_path(candidate.id).display().to_string()
        } else {
            "source-backed manifest verified".to_string()
        },
        event_id
    );
    Ok(())
}

struct ManifestCounts {
    total: usize,
    candidate: usize,
    unverified: usize,
    verified: usize,
}

impl ManifestCounts {
    fn from_candidates() -> Self {
        let mut counts = ManifestCounts {
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

impl CandidateStatus {
    fn label(self) -> &'static str {
        match self {
            CandidateStatus::Candidate => "candidate",
            CandidateStatus::Unverified => "unverified",
            CandidateStatus::Verified => "verified",
        }
    }
}

fn find_candidate(id: &str) -> Result<&'static ModelManifestEntry, AppError> {
    CANDIDATES
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| {
            AppError::usage(format!(
                "알 수 없는 모델 id입니다: {id}\n사용 가능 후보는 `rpotato model list`로 확인하세요."
            ))
        })
}

fn install_ready_for_report(candidate: &'static ModelManifestEntry) -> bool {
    validate_install_ready(candidate).ready
        || local_promotion_readiness(candidate)
            .map(|readiness| readiness.validation.ready)
            .unwrap_or(false)
}

fn validate_install_ready(candidate: &ModelManifestEntry) -> InstallValidation {
    let mut blockers = Vec::new();

    if candidate.status != CandidateStatus::Verified {
        push_unique(
            &mut blockers,
            "manifest status가 verified가 아니므로 설치할 수 없습니다.",
        );
    }

    for blocker in candidate.install_blockers {
        push_unique(&mut blockers, blocker);
    }

    if candidate.artifact_provider.is_none() {
        push_unique(&mut blockers, "artifact provider 미확정");
    }
    if candidate.artifact_url.is_none() {
        push_unique(&mut blockers, "GGUF artifact URL 미확정");
    }
    if candidate.artifact_terms_url.is_none() {
        push_unique(&mut blockers, "artifact terms URL 미확정");
    }
    if candidate.artifact_name.is_none() {
        push_unique(&mut blockers, "artifact file name 미확정");
    }
    if candidate.quantization.is_none() {
        push_unique(&mut blockers, "quantization 미확정");
    }
    if candidate.size_bytes.is_none() {
        push_unique(&mut blockers, "file size 미확정");
    }
    if candidate.backend_compatibility.is_none() {
        push_unique(&mut blockers, "backend compatibility source 미확정");
    }
    if candidate.recommended_ram_gb.is_none() {
        push_unique(&mut blockers, "recommended RAM source 미확정");
    }

    match candidate.sha256 {
        Some(hash) if checksum::is_valid_sha256(hash) => {}
        Some(_) => push_unique(&mut blockers, "SHA-256 형식 오류"),
        None => push_unique(&mut blockers, "SHA-256 미확정"),
    }

    InstallValidation {
        ready: blockers.is_empty(),
        blockers,
    }
}

fn source_backed_artifact(
    candidate: &'static ModelManifestEntry,
) -> Result<ModelArtifactDescriptor, AppError> {
    let blockers = source_backed_artifact_blockers(candidate);
    if !blockers.is_empty() {
        return Err(fetch_blocked(candidate, blockers));
    }

    Ok(ModelArtifactDescriptor {
        provider: candidate
            .artifact_provider
            .expect("validated artifact provider"),
        url: candidate.artifact_url.expect("validated artifact url"),
        terms_url: candidate
            .artifact_terms_url
            .expect("validated artifact terms url"),
        file_name: candidate.artifact_name.expect("validated artifact name"),
        sha256: candidate.sha256.expect("validated artifact sha256"),
        size_bytes: candidate.size_bytes.expect("validated artifact size"),
    })
}

fn source_backed_artifact_blockers(candidate: &ModelManifestEntry) -> Vec<&'static str> {
    let mut blockers = Vec::new();

    if candidate.artifact_provider.is_none() {
        blockers.push("artifact provider 미확정");
    }
    if candidate.artifact_url.is_none() {
        blockers.push("GGUF artifact URL 미확정");
    }
    if candidate.artifact_terms_url.is_none() {
        blockers.push("artifact terms URL 미확정");
    }
    if candidate.artifact_name.is_none() {
        blockers.push("artifact file name 미확정");
    }
    match candidate.sha256 {
        Some(hash) if checksum::is_valid_sha256(hash) => {}
        Some(_) => blockers.push("SHA-256 형식 오류"),
        None => blockers.push("SHA-256 미확정"),
    }
    if candidate.size_bytes.is_none() {
        blockers.push("file size 미확정");
    }
    if candidate.format != "gguf" {
        blockers.push("GGUF format이 아닙니다");
    }
    if candidate.backend != "llama.cpp" {
        blockers.push("llama.cpp backend 후보가 아닙니다");
    }

    blockers
}

fn fetch_blocked(candidate: &ModelManifestEntry, blockers: Vec<&str>) -> AppError {
    AppError::blocked(format!(
        "검증용 model artifact fetch 차단\n- id: {}\n- status: {}\n- blockers: {}\n- 동작: source-backed artifact URL, terms, size, SHA-256이 모두 있어야 검증용 fetch를 실행합니다.",
        candidate.id,
        candidate.status.label(),
        blockers.join(", ")
    ))
}

fn local_artifact_state(
    artifact: ModelArtifactDescriptor,
    final_path: &Path,
) -> Result<LocalArtifactState, AppError> {
    if !final_path.exists() {
        return Ok(LocalArtifactState {
            status: "missing",
            detail: "final artifact file is not present under app data models/".to_string(),
            verified: false,
        });
    }
    if !final_path.is_file() {
        return Ok(LocalArtifactState {
            status: "path-not-file",
            detail: format!(
                "final artifact path is not a file: {}",
                final_path.display()
            ),
            verified: false,
        });
    }

    let metadata = final_path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "model artifact metadata를 읽지 못했습니다: {} ({err})",
            final_path.display()
        ))
    })?;
    if metadata.len() != artifact.size_bytes {
        return Ok(LocalArtifactState {
            status: "size-mismatch",
            detail: format!(
                "expected {} bytes but found {} bytes",
                artifact.size_bytes,
                metadata.len()
            ),
            verified: false,
        });
    }

    let actual_sha256 = checksum::sha256_file(final_path)?;
    if !actual_sha256.eq_ignore_ascii_case(artifact.sha256) {
        return Ok(LocalArtifactState {
            status: "sha256-mismatch",
            detail: format!("expected {} but found {}", artifact.sha256, actual_sha256),
            verified: false,
        });
    }

    Ok(LocalArtifactState {
        status: "verified-local-artifact",
        detail: "size and SHA-256 match the source-recorded manifest fields".to_string(),
        verified: true,
    })
}

fn fetch_evaluation_artifact(
    artifact: ModelArtifactDescriptor,
    final_path: &Path,
    part_path: &Path,
) -> Result<ModelArtifactFetchStatus, AppError> {
    if final_path.exists() && !final_path.is_file() {
        return Err(AppError::blocked(format!(
            "model artifact final path가 file이 아닙니다: {}",
            final_path.display()
        )));
    }
    if final_path.is_file() {
        if model_artifact_matches(artifact, final_path)? {
            return Ok(ModelArtifactFetchStatus::CacheHit);
        }
        return Err(AppError::blocked(format!(
            "기존 model artifact가 manifest와 일치하지 않아 덮어쓰지 않습니다.\n- path: {}\n- expected size: {}\n- expected sha256: {}\n- 다음 단계: 파일을 수동으로 이동하거나 삭제한 뒤 다시 실행하세요.",
            final_path.display(),
            artifact.size_bytes,
            artifact.sha256
        )));
    }

    let final_parent = final_path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "model artifact final parent path를 계산하지 못했습니다: {}",
            final_path.display()
        ))
    })?;
    let part_parent = part_path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "model artifact partial parent path를 계산하지 못했습니다: {}",
            part_path.display()
        ))
    })?;
    fs::create_dir_all(final_parent).map_err(|err| {
        AppError::runtime(format!(
            "model artifact directory를 만들지 못했습니다: {} ({err})",
            final_parent.display()
        ))
    })?;
    fs::create_dir_all(part_parent).map_err(|err| {
        AppError::runtime(format!(
            "model artifact download directory를 만들지 못했습니다: {} ({err})",
            part_parent.display()
        ))
    })?;

    let existing_bytes = partial_artifact_size(part_path, artifact)?;
    if existing_bytes == artifact.size_bytes {
        verify_model_artifact_file(artifact, part_path)?;
        place_verified_artifact(part_path, final_path)?;
        return Ok(ModelArtifactFetchStatus::Resumed);
    }

    let (start_offset, resumed) =
        download_model_artifact_stream(artifact, part_path, existing_bytes)?;
    verify_partial_size(part_path, artifact, start_offset)?;
    verify_model_artifact_file(artifact, part_path)?;
    place_verified_artifact(part_path, final_path)?;

    if resumed {
        Ok(ModelArtifactFetchStatus::Resumed)
    } else {
        Ok(ModelArtifactFetchStatus::Downloaded)
    }
}

fn partial_artifact_size(
    part_path: &Path,
    artifact: ModelArtifactDescriptor,
) -> Result<u64, AppError> {
    if !part_path.exists() {
        return Ok(0);
    }
    if !part_path.is_file() {
        return Err(AppError::blocked(format!(
            "model artifact partial path가 file이 아닙니다: {}",
            part_path.display()
        )));
    }

    let size = part_path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "model artifact partial metadata를 읽지 못했습니다: {} ({err})",
                part_path.display()
            ))
        })?
        .len();
    if size > artifact.size_bytes {
        return Err(AppError::blocked(format!(
            "model artifact partial size가 manifest보다 큽니다.\n- expected: {}\n- actual: {}\n- path: {}\n- 다음 단계: rpotato model cleanup-failed <id> --delete 로 app-managed partial을 정리하세요.",
            artifact.size_bytes,
            size,
            part_path.display()
        )));
    }

    Ok(size)
}

fn download_model_artifact_stream(
    artifact: ModelArtifactDescriptor,
    part_path: &Path,
    existing_bytes: u64,
) -> Result<(u64, bool), AppError> {
    let mut request = ureq::get(artifact.url)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")));
    if existing_bytes > 0 {
        request = request.header("Range", &format!("bytes={existing_bytes}-"));
    }

    let response = request.call().map_err(|err| {
        AppError::runtime(format!(
            "model artifact 다운로드 실패\n- url: {}\n- error: {err}",
            artifact.url
        ))
    })?;
    let status_code = response.status().as_u16();
    let (start_offset, resumed) = match (existing_bytes, status_code) {
        (0, 200 | 206) => (0, false),
        (_, 206) => (existing_bytes, true),
        (_, 200) => (0, false),
        (_, status) => {
            return Err(AppError::blocked(format!(
                "model artifact 다운로드 HTTP status가 예상과 다릅니다.\n- url: {}\n- status: {}\n- expected: 200 또는 206",
                artifact.url, status
            )));
        }
    };

    let (_, body) = response.into_parts();
    let mut reader = body.into_reader();
    let mut file: Box<dyn Write> = if start_offset == 0 {
        Box::new(File::create(part_path).map_err(|err| {
            AppError::runtime(format!(
                "model artifact partial file을 만들지 못했습니다: {} ({err})",
                part_path.display()
            ))
        })?)
    } else {
        Box::new(
            OpenOptions::new()
                .append(true)
                .open(part_path)
                .map_err(|err| {
                    AppError::runtime(format!(
                        "model artifact partial file을 append로 열지 못했습니다: {} ({err})",
                        part_path.display()
                    ))
                })?,
        )
    };

    copy_model_reader_with_limit(&mut reader, &mut file, start_offset, artifact.size_bytes)?;
    Ok((start_offset, resumed))
}

fn verify_partial_size(
    part_path: &Path,
    artifact: ModelArtifactDescriptor,
    start_offset: u64,
) -> Result<(), AppError> {
    let actual_bytes = part_path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "model artifact partial metadata를 읽지 못했습니다: {} ({err})",
                part_path.display()
            ))
        })?
        .len();
    if actual_bytes != artifact.size_bytes {
        return Err(AppError::blocked(format!(
            "model artifact size 검증 실패\n- expected: {}\n- actual: {}\n- resumed from: {}\n- path: {}\n- 동작: partial은 보존되며 같은 명령으로 재시도하거나 cleanup-failed로 정리할 수 있습니다.",
            artifact.size_bytes,
            actual_bytes,
            start_offset,
            part_path.display()
        )));
    }

    Ok(())
}

fn copy_model_reader_with_limit<R: Read, W: Write + ?Sized>(
    reader: &mut R,
    writer: &mut W,
    existing_bytes: u64,
    expected_total_bytes: u64,
) -> Result<u64, AppError> {
    let mut copied_bytes = 0_u64;
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_BYTES];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|err| AppError::runtime(format!("model artifact stream read 실패: {err}")))?;
        if bytes_read == 0 {
            break;
        }
        copied_bytes += bytes_read as u64;
        let total_bytes = existing_bytes + copied_bytes;
        if total_bytes > expected_total_bytes {
            return Err(AppError::blocked(format!(
                "model artifact size limit 초과\n- expected: {}\n- actual-at-least: {}",
                expected_total_bytes, total_bytes
            )));
        }
        writer.write_all(&buffer[..bytes_read]).map_err(|err| {
            AppError::runtime(format!("model artifact partial file write 실패: {err}"))
        })?;
    }

    writer
        .flush()
        .map_err(|err| AppError::runtime(format!("model artifact partial flush 실패: {err}")))?;
    Ok(copied_bytes)
}

fn model_artifact_matches(
    artifact: ModelArtifactDescriptor,
    path: &Path,
) -> Result<bool, AppError> {
    let metadata = path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "model artifact metadata를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "model artifact path가 file이 아닙니다: {}",
            path.display()
        )));
    }
    if metadata.len() != artifact.size_bytes {
        return Ok(false);
    }

    let actual_sha256 = checksum::sha256_file(path)?;
    Ok(actual_sha256.eq_ignore_ascii_case(artifact.sha256))
}

fn verify_model_artifact_file(
    artifact: ModelArtifactDescriptor,
    path: &Path,
) -> Result<(), AppError> {
    let metadata = path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "model artifact metadata를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "model artifact path가 file이 아닙니다: {}",
            path.display()
        )));
    }
    if metadata.len() != artifact.size_bytes {
        return Err(AppError::blocked(format!(
            "model artifact size 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.size_bytes,
            metadata.len(),
            path.display()
        )));
    }

    let actual_sha256 = checksum::sha256_file(path)?;
    if !actual_sha256.eq_ignore_ascii_case(artifact.sha256) {
        return Err(AppError::blocked(format!(
            "model artifact SHA-256 검증 실패\n- expected: {}\n- actual: {}\n- path: {}\n- 동작: registry 등록은 수행하지 않으며 partial은 cleanup-failed 대상으로 남깁니다.",
            artifact.sha256,
            actual_sha256,
            path.display()
        )));
    }

    Ok(())
}

fn place_verified_artifact(part_path: &Path, final_path: &Path) -> Result<(), AppError> {
    if final_path.exists() {
        return Err(AppError::blocked(format!(
            "model artifact final path가 이미 존재해 partial을 배치하지 않습니다: {}",
            final_path.display()
        )));
    }

    fs::rename(part_path, final_path).map_err(|err| {
        AppError::runtime(format!(
            "model artifact 배치 실패: {} -> {} ({err})",
            part_path.display(),
            final_path.display()
        ))
    })
}

fn model_artifact_path(artifact: ModelArtifactDescriptor) -> PathBuf {
    paths::models_dir().join(artifact.file_name)
}

fn model_artifact_part_path(candidate: &ModelManifestEntry) -> PathBuf {
    paths::downloads_dir().join(format!("{}.part", candidate.id))
}

impl ModelArtifactFetchStatus {
    fn label(self) -> &'static str {
        match self {
            ModelArtifactFetchStatus::Downloaded => "downloaded",
            ModelArtifactFetchStatus::Resumed => "resumed",
            ModelArtifactFetchStatus::CacheHit => "cache-hit",
        }
    }
}

fn persist_promotion_evidence(
    candidate: &ModelManifestEntry,
    evidence: &PromotionEvidence,
    artifact: ModelArtifactDescriptor,
    benchmark: &observability::BenchmarkRunReport,
    evidence_source: &Path,
) -> Result<(), AppError> {
    fs::create_dir_all(paths::model_evidence_dir()).map_err(|err| {
        AppError::runtime(format!(
            "model evidence directory를 만들지 못했습니다: {} ({err})",
            paths::model_evidence_dir().display()
        ))
    })?;

    fs::write(
        promotion_evidence_path(candidate.id),
        promotion_evidence_json(candidate, evidence, artifact, benchmark, evidence_source),
    )
    .map_err(|err| {
        AppError::runtime(format!(
            "model promotion evidence를 기록하지 못했습니다: {} ({err})",
            promotion_evidence_path(candidate.id).display()
        ))
    })
}

fn persist_registry_entry(
    candidate: &ModelManifestEntry,
    promotion: Option<&PromotionEvidence>,
) -> Result<(), AppError> {
    fs::create_dir_all(paths::model_registry_dir()).map_err(|err| {
        AppError::runtime(format!(
            "model registry directory를 만들지 못했습니다: {} ({err})",
            paths::model_registry_dir().display()
        ))
    })?;

    fs::write(
        registry_path(candidate.id),
        registry_entry_json(candidate, promotion),
    )
    .map_err(|err| {
        AppError::runtime(format!(
            "model registry entry를 기록하지 못했습니다: {} ({err})",
            registry_path(candidate.id).display()
        ))
    })
}

fn registry_summary() -> String {
    match read_registry_entries() {
        Ok(entries) if entries.is_empty() => format!(
            "model registry\n- installed models: 0\n- registry dir: {}",
            paths::model_registry_dir().display()
        ),
        Ok(entries) => {
            let rows = entries
                .iter()
                .map(|entry| {
                    format!(
                        "- {} | status: {} | sha256: {} | path: {}",
                        entry.id, entry.status, entry.artifact_sha256, entry.artifact_path
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "model registry\n- installed models: {}\n- registry dir: {}\n{}",
                entries.len(),
                paths::model_registry_dir().display(),
                rows
            )
        }
        Err(err) => format!(
            "model registry\n- 상태: registry 읽기 실패\n- 이유: {}\n- registry dir: {}",
            err.message,
            paths::model_registry_dir().display()
        ),
    }
}

fn read_registry_entries() -> Result<Vec<RegistryEntry>, AppError> {
    let dir = paths::model_registry_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|err| {
        AppError::runtime(format!(
            "model registry directory를 읽지 못했습니다: {} ({err})",
            dir.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "model registry entry를 읽지 못했습니다: {} ({err})",
                dir.display()
            ))
        })?;

        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }

        let text = fs::read_to_string(entry.path()).map_err(|err| {
            AppError::runtime(format!(
                "model registry entry를 읽지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;

        if let Some(registry_entry) = parse_registry_entry(&text) {
            entries.push(registry_entry);
        }
    }

    entries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(entries)
}

fn parse_registry_entry(text: &str) -> Option<RegistryEntry> {
    Some(RegistryEntry {
        id: extract_json_string(text, "id")?,
        display_name: extract_json_string(text, "displayName")?,
        status: extract_json_string(text, "status")?,
        artifact_path: extract_json_string(text, "artifactPath")?,
        artifact_sha256: extract_json_string(text, "artifactSha256")?,
    })
}

fn registry_path(id: &str) -> PathBuf {
    paths::model_registry_dir().join(format!("{id}.json"))
}

fn promotion_evidence_path(id: &str) -> PathBuf {
    paths::model_evidence_dir().join(format!("{id}.promotion.json"))
}

fn failed_artifact_paths(candidate: &ModelManifestEntry) -> Vec<PathBuf> {
    let artifact_name = candidate.artifact_name.unwrap_or(candidate.id);
    vec![
        paths::downloads_dir().join(format!("{}.part", candidate.id)),
        paths::downloads_dir().join(format!("{}.failed", candidate.id)),
        paths::models_dir().join(format!("{artifact_name}.failed")),
    ]
}

fn registry_entry_json(
    candidate: &ModelManifestEntry,
    promotion: Option<&PromotionEvidence>,
) -> String {
    let evidence_status = if promotion.is_some() {
        "verified-local-promotion"
    } else {
        "source-backed-manifest"
    };
    let evidence_path = if promotion.is_some() {
        promotion_evidence_path(candidate.id).display().to_string()
    } else {
        String::new()
    };
    let backend_version = promotion
        .map(|evidence| evidence.backend_version.as_str())
        .unwrap_or("");
    let benchmark_run_id = promotion
        .map(|evidence| evidence.benchmark_run_id.as_str())
        .unwrap_or("");
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"status\": \"installed\",\n  \"evidenceStatus\": \"{}\",\n  \"promotionEvidencePath\": \"{}\",\n  \"backendVersion\": \"{}\",\n  \"benchmarkRunId\": \"{}\",\n  \"upstreamModel\": \"{}\",\n  \"upstreamUrl\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"licenseSource\": \"{}\",\n  \"licenseCheckedAt\": \"{}\"\n}}\n",
        ledger::json_string(candidate.id),
        ledger::json_string(candidate.display_name),
        ledger::json_string(evidence_status),
        ledger::json_string(&evidence_path),
        ledger::json_string(backend_version),
        ledger::json_string(benchmark_run_id),
        ledger::json_string(candidate.upstream_model),
        ledger::json_string(candidate.upstream_url),
        ledger::json_string(
            &paths::models_dir()
                .join(candidate.artifact_name.unwrap_or(candidate.id))
                .display()
                .to_string()
        ),
        ledger::json_string(candidate.sha256.unwrap_or("")),
        ledger::json_string(candidate.license.source),
        ledger::json_string(candidate.license.checked_at)
    )
}

fn promotion_evidence_json(
    candidate: &ModelManifestEntry,
    evidence: &PromotionEvidence,
    artifact: ModelArtifactDescriptor,
    benchmark: &observability::BenchmarkRunReport,
    evidence_source: &Path,
) -> String {
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"status\": \"verified-local-promotion\",\n  \"modelId\": \"{}\",\n  \"displayName\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"artifactSizeBytes\": {},\n  \"backendId\": \"{}\",\n  \"backendVersion\": \"{}\",\n  \"backendSmokeEventId\": \"{}\",\n  \"ramFit\": \"{}\",\n  \"recommendedRamGb\": {},\n  \"peakRssBytes\": {},\n  \"mmproj\": \"{}\",\n  \"benchmarkRunId\": \"{}\",\n  \"benchmarkName\": \"{}\",\n  \"benchmarkScore\": {},\n  \"benchmarkLocalPass\": {},\n  \"sourceEvidencePath\": \"{}\",\n  \"recordedAt\": \"{}\"\n}}\n",
        ledger::json_string(candidate.id),
        ledger::json_string(candidate.display_name),
        ledger::json_string(&model_artifact_path(artifact).display().to_string()),
        ledger::json_string(&evidence.artifact_sha256),
        evidence.artifact_size_bytes,
        ledger::json_string(&evidence.backend_id),
        ledger::json_string(&evidence.backend_version),
        ledger::json_string(&evidence.backend_smoke_event_id),
        ledger::json_string(&evidence.ram_fit),
        evidence.recommended_ram_gb,
        evidence.peak_rss_bytes,
        ledger::json_string(&evidence.mmproj),
        ledger::json_string(&benchmark.benchmark_run_id),
        ledger::json_string(&benchmark.benchmark_name),
        benchmark
            .score
            .map(|score| format!("{score:.6}"))
            .unwrap_or_else(|| "null".to_string()),
        benchmark
            .local_pass
            .map(|value| if value { "true" } else { "false" })
            .unwrap_or("null"),
        ledger::json_string(&evidence_source.display().to_string()),
        ledger::json_string(&evidence.recorded_at)
    )
}

fn read_promotion_evidence_file(path: &Path) -> Result<PromotionEvidence, AppError> {
    let text = fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "model promotion evidence를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_promotion_evidence(&text)
}

fn parse_promotion_evidence(text: &str) -> Result<PromotionEvidence, AppError> {
    let schema_version = required_json_u64(text, "schemaVersion")?;
    if schema_version != 1 {
        return Err(AppError::usage(format!(
            "model promotion evidence schemaVersion은 1이어야 합니다: {schema_version}"
        )));
    }

    let artifact_sha256 = required_json_string(text, "artifactSha256")?;
    if !checksum::is_valid_sha256(&artifact_sha256) {
        return Err(AppError::usage(
            "model promotion evidence artifactSha256은 64자리 hex string이어야 합니다.",
        ));
    }

    Ok(PromotionEvidence {
        model_id: required_json_string(text, "modelId")?,
        artifact_sha256,
        artifact_size_bytes: required_json_u64(text, "artifactSizeBytes")?,
        backend_id: required_json_string(text, "backendId")?,
        backend_version: required_json_string(text, "backendVersion")?,
        backend_smoke_event_id: required_json_string(text, "backendSmokeEventId")?,
        ram_fit: required_json_string(text, "ramFit")?,
        recommended_ram_gb: required_json_u32(text, "recommendedRamGb")?,
        peak_rss_bytes: required_json_u64(text, "peakRssBytes")?,
        mmproj: required_json_string(text, "mmproj")?,
        benchmark_run_id: required_json_string(text, "benchmarkRunId")?,
        recorded_at: required_json_string(text, "recordedAt")?,
    })
}

fn required_json_string(text: &str, key: &str) -> Result<String, AppError> {
    extract_json_string(text, key).ok_or_else(|| {
        AppError::usage(format!(
            "model promotion evidence에 필수 string field가 없습니다: {key}"
        ))
    })
}

fn required_json_u64(text: &str, key: &str) -> Result<u64, AppError> {
    extract_json_u64(text, key).ok_or_else(|| {
        AppError::usage(format!(
            "model promotion evidence에 필수 number field가 없습니다: {key}"
        ))
    })
}

fn required_json_u32(text: &str, key: &str) -> Result<u32, AppError> {
    let value = required_json_u64(text, key)?;
    u32::try_from(value).map_err(|_| {
        AppError::usage(format!(
            "model promotion evidence number field가 u32 범위를 넘습니다: {key}"
        ))
    })
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

fn extract_json_string(text: &str, key: &str) -> Option<String> {
    let raw_value = json_value_after_key(text, key)?.strip_prefix('"')?;
    let mut parsed = String::new();
    let mut escaped = false;

    for ch in raw_value.chars() {
        if escaped {
            match ch {
                '"' => parsed.push('"'),
                '\\' => parsed.push('\\'),
                'n' => parsed.push('\n'),
                'r' => parsed.push('\r'),
                't' => parsed.push('\t'),
                other => parsed.push(other),
            }
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(parsed),
            other => parsed.push(other),
        }
    }

    None
}

fn extract_json_u64(text: &str, key: &str) -> Option<u64> {
    let value = json_value_after_key(text, key)?;
    let digits = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }

    digits.parse().ok()
}

fn json_value_after_key<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let quoted_key = format!("\"{key}\"");
    let key_start = text.find(&quoted_key)?;
    let after_key = &text[key_start + quoted_key.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_summary_reports_verified_count() {
        let summary = candidate_summary();
        assert!(summary.contains("3개 후보"));
        assert!(summary.contains("verified 0개"));
    }

    #[test]
    fn manifest_validation_blocks_unverified_artifact_candidate() {
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let validation = validate_install_ready(candidate);

        assert!(!validation.ready);
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("verified")));
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("smoke")));
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("RAM")));
    }

    #[test]
    fn manifest_report_names_required_source_backed_fields() {
        let report = manifest_report();
        assert!(report.contains("artifactUrl"));
        assert!(report.contains("sha256"));
        assert!(report.contains("benchmark ledger"));
    }

    #[test]
    fn download_plan_blocks_candidate_without_verified_artifact() {
        let report = download_plan_report("qwen3.5-4b").unwrap();
        assert!(report.contains("status: blocked"));
        assert!(report.contains("license source"));
    }

    #[test]
    fn evaluation_fetch_accepts_source_backed_unverified_candidate() {
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();

        assert_eq!(artifact.provider, "unsloth/Qwen3.5-4B-GGUF");
        assert_eq!(artifact.file_name, "Qwen3.5-4B-Q4_K_M.gguf");
        assert!(checksum::is_valid_sha256(artifact.sha256));
    }

    #[test]
    fn evaluation_fetch_blocks_candidate_without_artifact_source() {
        let err = source_backed_artifact(find_candidate("qwen3.5-9b").unwrap()).unwrap_err();

        assert_eq!(err.code, 3);
        assert!(err.message.contains("fetch 차단"));
        assert!(err.message.contains("artifact provider"));
    }

    #[test]
    fn evaluation_fetch_paths_stay_under_app_data() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let data_root =
            std::env::temp_dir().join(format!("rpotato-fetch-path-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_DATA_HOME", &data_root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

        let candidate = find_candidate("gemma-4-e4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();
        let final_path = model_artifact_path(artifact);
        let part_path = model_artifact_part_path(candidate);

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert!(final_path.starts_with(data_root.join("models")));
        assert!(part_path.starts_with(data_root.join("downloads")));
        assert!(part_path.ends_with("gemma-4-e4b.part"));
    }

    #[test]
    fn eval_plan_reports_missing_local_artifact_without_download() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let data_root =
            std::env::temp_dir().join(format!("rpotato-eval-plan-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_DATA_HOME", &data_root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

        let report = eval_plan_report("qwen3.5-4b").unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert!(report.contains("blocked-before-backend-smoke"));
        assert!(report.contains("local artifact status: missing"));
        assert!(report.contains("local benchmark status: not-run"));
        assert!(report.contains("fetch-candidate qwen3.5-4b --for-evaluation"));
    }

    #[test]
    fn local_benchmark_status_reports_measured_qwen_row() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let data_root = std::env::temp_dir().join(format!(
            "rpotato-benchmark-status-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&data_root);
        std::env::set_var("RPOTATO_DATA_HOME", &data_root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

        observability::record_benchmark_run(&observability::BenchmarkRunMetric {
            benchmark_run_id: "benchmark-qwen-smoke".to_string(),
            session_id: "session-test".to_string(),
            model_run_id: Some("model-run-test".to_string()),
            model_id: "Qwen3.5-4B-Q4_K_M".to_string(),
            benchmark_name: "ontology-view-smoke".to_string(),
            fixture_id: "executable-smoke".to_string(),
            fixture_sha256: "fixture-sha".to_string(),
            prompt_artifact_sha256: Some("prompt-sha".to_string()),
            prompt_chars: Some(147),
            claim_state: "measured-locally".to_string(),
            score: Some(3.0),
            score_unit: Some("0-3-local-product-score".to_string()),
            local_pass: Some(true),
            expected_matches: Some(1),
            expected_total: Some(1),
            forbidden_matches: Some(0),
            harness_ref: "rpotato-benchmark-harness@test".to_string(),
            dataset_ref: Some("local-executable-smoke".to_string()),
            backend_id: Some("llama.cpp".to_string()),
            latency_ms: Some(243.0),
            tokens_per_second: Some(28.8),
            prompt_tokens: Some(76),
            completion_tokens: Some(7),
            total_tokens: Some(83),
            resource_pressure: Some("normal".to_string()),
            peak_rss_bytes: Some(3_351_363_584),
            reproducibility_manifest: "{}".to_string(),
            redacted_report: "{}".to_string(),
            recorded_at_ms: 1000,
        })
        .unwrap();

        let artifact = source_backed_artifact(find_candidate("qwen3.5-4b").unwrap()).unwrap();
        let status = local_benchmark_status(artifact).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(data_root);

        assert!(status.contains("measured-locally"));
        assert!(status.contains("latest_run=benchmark-qwen-smoke"));
        assert!(status.contains("score=3.000000"));
        assert!(status.contains("local_pass=true"));
    }

    #[test]
    fn promotion_evidence_parser_accepts_pretty_json() {
        let text = r#"{
  "schemaVersion": 1,
  "modelId": "qwen3.5-4b",
  "artifactSha256": "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
  "artifactSizeBytes": 2740937888,
  "backendId": "llama.cpp",
  "backendVersion": "b9878",
  "backendSmokeEventId": "event-backend-chat",
  "ramFit": "passed",
  "recommendedRamGb": 16,
  "peakRssBytes": 3351363584,
  "mmproj": "not-required-text-only",
  "benchmarkRunId": "benchmark-local",
  "recordedAt": "2026-07-10T00:00:00Z"
}"#;

        let evidence = parse_promotion_evidence(text).unwrap();

        assert_eq!(evidence.model_id, "qwen3.5-4b");
        assert_eq!(evidence.backend_version, "b9878");
        assert_eq!(evidence.recommended_ram_gb, 16);
    }

    #[test]
    fn promotion_evidence_validation_accepts_measured_local_benchmark() {
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();
        let evidence = qwen_promotion_evidence(artifact);
        let benchmark = qwen_benchmark_report(artifact, &evidence);
        let local_state = LocalArtifactState {
            status: "verified-local-artifact",
            detail: "test artifact verified".to_string(),
            verified: true,
        };

        let validation = validate_promotion_evidence(
            candidate,
            &evidence,
            artifact,
            &local_state,
            Some(&benchmark),
            true,
        );

        assert!(validation.ready, "{:?}", validation.blockers);
    }

    #[test]
    fn promotion_evidence_validation_blocks_ram_and_benchmark_gaps() {
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();
        let mut evidence = qwen_promotion_evidence(artifact);
        evidence.ram_fit = "unknown".to_string();
        evidence.peak_rss_bytes = 20 * BYTES_PER_GIB;
        let local_state = LocalArtifactState {
            status: "verified-local-artifact",
            detail: "test artifact verified".to_string(),
            verified: true,
        };

        let validation =
            validate_promotion_evidence(candidate, &evidence, artifact, &local_state, None, false);

        assert!(!validation.ready);
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("ramFit")));
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("recommendedRamGb")));
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("benchmarkRunId")));
        assert!(validation
            .blockers
            .iter()
            .any(|blocker| blocker.contains("backend smoke")));
    }

    #[test]
    fn registry_parser_accepts_pretty_json_entries() {
        let text = registry_entry_json(find_candidate("qwen3.5-4b").unwrap(), None);
        let entry = parse_registry_entry(&text).unwrap();

        assert_eq!(entry.id, "qwen3.5-4b");
        assert_eq!(entry.status, "installed");
        assert!(entry.artifact_sha256.starts_with("00fe"));
    }

    #[test]
    fn eval_plan_blocks_candidate_without_artifact_source() {
        let report = eval_plan_report("qwen3.5-9b").unwrap();

        assert!(report.contains("blocked-before-artifact-fetch"));
        assert!(report.contains("artifact provider"));
        assert!(report.contains("benchmark source"));
    }

    #[test]
    fn benchmark_plan_separates_public_and_local_conditions() {
        let report = benchmark_plan_report("qwen3.5-4b").unwrap();

        assert!(report.contains("public benchmark parity status"));
        assert!(report.contains("blocked-until-conditions-fixed"));
        assert!(report.contains("local product benchmark suite"));
        assert!(report.contains("published-vs-local rule"));
    }

    fn qwen_promotion_evidence(artifact: ModelArtifactDescriptor) -> PromotionEvidence {
        PromotionEvidence {
            model_id: "qwen3.5-4b".to_string(),
            artifact_sha256: artifact.sha256.to_string(),
            artifact_size_bytes: artifact.size_bytes,
            backend_id: "llama.cpp".to_string(),
            backend_version: "b9878".to_string(),
            backend_smoke_event_id: "event-backend-chat".to_string(),
            ram_fit: "passed".to_string(),
            recommended_ram_gb: 16,
            peak_rss_bytes: 3_351_363_584,
            mmproj: "not-required-text-only".to_string(),
            benchmark_run_id: "benchmark-local".to_string(),
            recorded_at: "2026-07-10T00:00:00Z".to_string(),
        }
    }

    fn qwen_benchmark_report(
        artifact: ModelArtifactDescriptor,
        evidence: &PromotionEvidence,
    ) -> observability::BenchmarkRunReport {
        observability::BenchmarkRunReport {
            benchmark_run_id: evidence.benchmark_run_id.clone(),
            session_id: "session-test".to_string(),
            model_run_id: Some("model-run-test".to_string()),
            model_id: artifact_model_id(artifact),
            benchmark_name: "ontology-view-smoke".to_string(),
            fixture_id: "fixture-test".to_string(),
            fixture_sha256: "fixture-sha".to_string(),
            prompt_artifact_sha256: Some("prompt-sha".to_string()),
            prompt_chars: Some(147),
            claim_state: "measured-locally".to_string(),
            score: Some(3.0),
            score_unit: Some("0-3-local-product-score".to_string()),
            local_pass: Some(true),
            expected_matches: Some(1),
            expected_total: Some(1),
            forbidden_matches: Some(0),
            harness_ref: "rpotato-benchmark-harness@test".to_string(),
            dataset_ref: Some("local-executable-smoke".to_string()),
            backend_id: Some("llama.cpp".to_string()),
            latency_ms: Some(243.0),
            tokens_per_second: Some(28.8),
            prompt_tokens: Some(76),
            completion_tokens: Some(7),
            total_tokens: Some(83),
            resource_pressure: Some("normal".to_string()),
            peak_rss_bytes: Some(evidence.peak_rss_bytes),
            reproducibility_manifest: "{}".to_string(),
            redacted_report: "{}".to_string(),
            recorded_at_ms: 1000,
        }
    }

    #[test]
    fn cleanup_failed_dry_run_lists_app_managed_paths() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let data_root =
            std::env::temp_dir().join(format!("rpotato-cleanup-test-{}", std::process::id()));
        std::env::set_var("RPOTATO_DATA_HOME", &data_root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

        let report = cleanup_failed_report("qwen3.5-4b", true).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        assert!(report.contains("dry-run"));
        assert!(report.contains("qwen3.5-4b.part"));
        assert!(report.contains("app data downloads/models"));
    }
}
