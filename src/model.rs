use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::app::AppError;
use crate::{checksum, ledger, paths, state};

const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;

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
            .filter(|candidate| validate_install_ready(candidate).ready)
            .count(),
        paths::model_registry_dir().display()
    );

    for candidate in CANDIDATES {
        let validation = validate_install_ready(candidate);
        let install_state = if validation.ready {
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
        "model manifest schema\n- schemaVersion: 1\n- required status: {}\n- required source-backed fields: upstreamModel, upstreamUrl, license, licenseSource, licenseCheckedAt, artifactUrl, artifactProvider, artifactTermsUrl, sha256, sizeBytes, quantization, backendCompatibility, recommendedRamEvidence\n- benchmark ledger fields: publishedScoreSource, checkedAt, harness, dataset, scoring, backend, quantization, contextLength, localScore, parityStatus\n- install gate: verified status plus valid SHA-256 and non-empty artifact/source/license/backend fields\n- local registry: app data models/registry/<model-id>.json\n- 금지: checksum 없는 설치, license 미표기 설치, 출처 없는 RAM/backend/benchmark claim 확정",
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
    Ok(format!(
        "model inspect\n- id: {}\n- display name: {}\n- status: {}\n- install ready: {}\n- blockers: {}\n- upstream model: {}\n- upstream source: {}\n- license claim: {}\n- license source: {}\n- license checked-at: {}\n- artifact provider: {}\n- artifact URL: {}\n- artifact terms: {}\n- artifact name: {}\n- format: {}\n- backend: {}\n- quantization: {}\n- sha256: {}\n- size bytes: {}\n- context length: {}\n- recommended RAM GB: {}\n- backend compatibility: {}\n- public benchmark source: {}\n- benchmark checked-at: {}\n- benchmark claim status: {}\n- benchmark harness: {}\n- benchmark dataset: {}\n- benchmark prompt: {}\n- benchmark scoring: {}\n- benchmark hardware/backend: {}\n- reproducibility: {}",
        candidate.id,
        candidate.display_name,
        candidate.status.label(),
        if validation.ready { "yes" } else { "no" },
        display_vec(&validation.blockers),
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
    let plan_status = if local_state.verified {
        "ready-for-backend-smoke"
    } else {
        "blocked-before-backend-smoke"
    };
    let next = if local_state.verified {
        format!(
            "run `rpotato backend install-plan`, verify backend state with `rpotato backend doctor`, then run `rpotato backend start --model {}` for local smoke before benchmark scoring.",
            final_path.display()
        )
    } else {
        format!(
            "run `rpotato model fetch-candidate {} --for-evaluation` only when intentionally downloading the multi-GB artifact.",
            candidate.id
        )
    };

    Ok(format!(
        "model evaluation plan\n- id: {}\n- status: {}\n- manifest status: {}\n- role: {}\n- artifact provider: {}\n- artifact source: {}\n- artifact terms: {}\n- expected file: {}\n- expected size bytes: {}\n- expected sha256: {}\n- local artifact status: {}\n- local artifact detail: {}\n- partial path: {}\n- final path: {}\n- registry: not installed by eval-plan\n- public benchmark source: {}\n- benchmark claim status: {}\n- local benchmark status: not-run\n- next: {}",
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
        next
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
        "검증용 model artifact 준비 완료\n- id: {}\n- status: {}\n- provider: {}\n- source: {}\n- terms: {}\n- file: {}\n- size bytes: {}\n- sha256: {}\n- partial path: {}\n- final path: {}\n- registry: not registered\n- ledger event: {}\n- 다음 단계: rpotato backend start --model {} 로 local smoke를 실행하고, benchmark/RAM-fit/mmproj evidence가 쌓인 뒤에만 verified 승격을 검토합니다.",
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
    let validation = validate_install_ready(candidate);

    if !validation.ready {
        let event_id = state::record_event(
            "model.install.blocked",
            "미검증 model install 차단",
            &format!(
                "model_id={} status={} blockers={}",
                candidate.id,
                candidate.status.label(),
                validation.blockers.join(",")
            ),
        )?;
        return Err(AppError::blocked(format!(
            "설치를 차단했습니다: {}\n상태: {}\n이유:\n- {}\nsource: {}\nlicense source: {}\nbenchmark source: {}\nlocal registry: {}\nledger event: {}\n다음 단계: source-recorded artifact field를 유지하면서 local backend smoke, RAM-fit/mmproj 측정, byte-level SHA-256 검증, benchmark evidence를 채운 뒤 verified 상태로 승격해야 합니다.",
            candidate.id,
            candidate.status.label(),
            validation.blockers.join("\n- "),
            candidate.upstream_url,
            candidate.license.source,
            candidate.benchmark.source,
            paths::model_registry_dir().display(),
            event_id
        )));
    }

    persist_registry_entry(candidate)?;
    let event_id = state::record_event(
        "model.install.registered",
        "검증된 model registry 등록",
        &format!("model_id={}", candidate.id),
    )?;

    println!(
        "모델 registry 등록 완료\n- id: {}\n- registry: {}\n- ledger event: {}\n- 동작: 다운로드 실행 경로는 이어받기와 checksum 검증 phase에서 연결합니다.",
        candidate.id,
        registry_path(candidate.id).display(),
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
    let mut request = ureq::get(artifact.url).header("User-Agent", "rpotato/0.1.0");
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

fn persist_registry_entry(candidate: &ModelManifestEntry) -> Result<(), AppError> {
    fs::create_dir_all(paths::model_registry_dir()).map_err(|err| {
        AppError::runtime(format!(
            "model registry directory를 만들지 못했습니다: {} ({err})",
            paths::model_registry_dir().display()
        ))
    })?;

    fs::write(registry_path(candidate.id), registry_entry_json(candidate)).map_err(|err| {
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

fn failed_artifact_paths(candidate: &ModelManifestEntry) -> Vec<PathBuf> {
    let artifact_name = candidate.artifact_name.unwrap_or(candidate.id);
    vec![
        paths::downloads_dir().join(format!("{}.part", candidate.id)),
        paths::downloads_dir().join(format!("{}.failed", candidate.id)),
        paths::models_dir().join(format!("{artifact_name}.failed")),
    ]
}

fn registry_entry_json(candidate: &ModelManifestEntry) -> String {
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"status\": \"installed\",\n  \"upstreamModel\": \"{}\",\n  \"upstreamUrl\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"licenseSource\": \"{}\",\n  \"licenseCheckedAt\": \"{}\"\n}}\n",
        ledger::json_string(candidate.id),
        ledger::json_string(candidate.display_name),
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

fn extract_json_string(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = line.find(&needle)? + needle.len();
    let mut value = String::new();
    let mut escaped = false;

    for ch in line[start..].chars() {
        if escaped {
            match ch {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => value.push(other),
            }
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(value),
            other => value.push(other),
        }
    }

    None
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
        assert!(report.contains("fetch-candidate qwen3.5-4b --for-evaluation"));
    }

    #[test]
    fn eval_plan_blocks_candidate_without_artifact_source() {
        let report = eval_plan_report("qwen3.5-9b").unwrap();

        assert!(report.contains("blocked-before-artifact-fetch"));
        assert!(report.contains("artifact provider"));
        assert!(report.contains("benchmark source"));
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
