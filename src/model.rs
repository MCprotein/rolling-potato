use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::model_artifact::{
    self, failed_artifact_paths, fetch_evaluation_artifact, local_artifact_state,
    model_artifact_part_path, model_artifact_path, promotion_evidence_path, read_default_selection,
    read_registry_entries, registry_path,
};
#[cfg(test)]
use crate::adapters::filesystem::model_artifact::{parse_default_selection, parse_registry_entry};
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::benchmark as benchmark_policy;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, source_backed_artifact_blockers,
    validate_install_ready, BackendSmokeEvidence, CandidateStatus, DefaultSelection,
    InstallValidation, LocalArtifactState, ManifestCounts, ModelArtifactDescriptor,
    ModelManifestEntry, PromotionEvidence, PromotionReadiness, RegistryEntry, CANDIDATES,
    STATUS_SCHEMA,
};
use crate::{ledger, observability, state};

const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;

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

pub fn registry_report() -> String {
    registry_summary()
}

pub fn default_report() -> Result<String, AppError> {
    let selection = read_default_selection()?;
    let entry = validated_registry_entry(&selection.model_id)?;
    if selection.artifact_sha256 != entry.artifact_sha256 {
        return Err(AppError::blocked(
            "기본 모델 선택의 artifact SHA-256이 registry와 다릅니다.",
        ));
    }

    Ok(format!(
        "기본 모델\n- id: {}\n- display name: {}\n- artifact: {}\n- sha256: {}\n- backend version: {}\n- benchmark run: {}\n- selected at ms: {}\n- 상태: registry, artifact, promotion evidence 재검증 완료",
        entry.id,
        entry.display_name,
        entry.artifact_path,
        entry.artifact_sha256,
        entry.backend_version,
        entry.benchmark_run_id,
        selection.selected_at_ms
    ))
}

pub fn set_default_report(id: &str) -> Result<String, AppError> {
    let entry = validated_registry_entry(id)?;
    let selection = DefaultSelection {
        model_id: entry.id.clone(),
        artifact_sha256: entry.artifact_sha256.clone(),
        selected_at_ms: now_ms_u64(),
    };
    let body = default_selection_json(&selection);
    state::atomic_replace_bytes(&model_artifact::paths().default_file, body.as_bytes())?;
    let event_id = state::record_event(
        "model.default.selected",
        "기본 모델 선택 완료",
        &format!(
            "model_id={} artifact_sha256={} registry={} selection={}",
            entry.id,
            entry.artifact_sha256,
            registry_path(&entry.id).display(),
            model_artifact::paths().default_file.display()
        ),
    )?;

    Ok(format!(
        "기본 모델 선택 완료\n- id: {}\n- artifact: {}\n- sha256: {}\n- selection: {}\n- ledger event: {}\n- 동작: backend start에서 --model을 생략하면 이 모델을 재검증한 뒤 사용합니다.",
        entry.id,
        entry.artifact_path,
        entry.artifact_sha256,
        model_artifact::paths().default_file.display(),
        event_id
    ))
}

pub fn default_artifact_path() -> Result<PathBuf, AppError> {
    let selection = read_default_selection()?;
    let entry = validated_registry_entry(&selection.model_id)?;
    if selection.artifact_sha256 != entry.artifact_sha256 {
        return Err(AppError::blocked(
            "기본 모델 선택의 artifact SHA-256이 registry와 다릅니다.",
        ));
    }
    Ok(PathBuf::from(entry.artifact_path))
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
    if !model_artifact::paths().observability_db_file.exists() {
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
    let backend_smoke = backend_smoke_evidence(&evidence.backend_smoke_event_id)?;
    let validation = validate_promotion_evidence(
        candidate,
        &evidence,
        artifact,
        &local_state,
        benchmark.as_ref(),
        backend_smoke.as_ref(),
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
    if !model_artifact::paths().observability_db_file.exists() {
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

fn backend_smoke_evidence(event_id: &str) -> Result<Option<BackendSmokeEvidence>, AppError> {
    if event_id.trim().is_empty() {
        return Ok(None);
    }
    let Some(event) = ledger::read_runtime_events()?
        .into_iter()
        .find(|event| event.event_id == event_id && event.event_type == "backend.chat.completed")
    else {
        return Ok(None);
    };
    let field = |key| detail_value(&event.details, key).map(str::to_string);
    let Some(model_size_bytes) = field("model_size_bytes").and_then(|value| value.parse().ok())
    else {
        return Ok(None);
    };
    let Some(evidence) = (|| {
        Some(BackendSmokeEvidence {
            event_id: event.event_id,
            backend_id: field("backend")?,
            backend_release: field("backend_release")?,
            binary_sha256: field("binary_sha256")?,
            model_id: field("model_id")?,
            model_sha256: field("model_sha256")?,
            model_size_bytes,
            ctx_size: field("ctx_size")?,
            mmproj: field("mmproj")?,
            sampling: field("sampling")?,
            host_os: field("host_os")?,
            host_arch: field("host_arch")?,
        })
    })() else {
        return Ok(None);
    };
    Ok(Some(evidence))
}

fn validate_promotion_evidence(
    candidate: &ModelManifestEntry,
    evidence: &PromotionEvidence,
    artifact: ModelArtifactDescriptor,
    local_state: &LocalArtifactState,
    benchmark: Option<&observability::BenchmarkRunReport>,
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

fn measured_ram_budget_gb(peak_rss_bytes: u64) -> u32 {
    let measured_gib = peak_rss_bytes.saturating_add(BYTES_PER_GIB - 1) / BYTES_PER_GIB;
    measured_gib.saturating_add(2).min(u64::from(u32::MAX)) as u32
}

pub(crate) fn quantization_for_artifact_hash(hash: &str) -> Option<&'static str> {
    CANDIDATES
        .iter()
        .find(|candidate| candidate.sha256 == Some(hash))
        .and_then(|candidate| candidate.quantization)
}

fn detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details.split_whitespace().find_map(|field| {
        let (candidate, value) = field.split_once('=')?;
        (candidate == key).then_some(value)
    })
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
    let backend_smoke = backend_smoke_evidence(&evidence.backend_smoke_event_id)?;
    let validation = validate_promotion_evidence(
        candidate,
        &evidence,
        artifact,
        &local_state,
        benchmark.as_ref(),
        backend_smoke.as_ref(),
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
            model_artifact::paths().registry_dir.display(),
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

fn install_ready_for_report(candidate: &'static ModelManifestEntry) -> bool {
    validate_install_ready(candidate).ready
        || local_promotion_readiness(candidate)
            .map(|readiness| readiness.validation.ready)
            .unwrap_or(false)
}

fn persist_promotion_evidence(
    candidate: &ModelManifestEntry,
    evidence: &PromotionEvidence,
    artifact: ModelArtifactDescriptor,
    benchmark: &observability::BenchmarkRunReport,
    evidence_source: &Path,
) -> Result<(), AppError> {
    model_artifact::write_promotion_evidence(
        candidate.id,
        &promotion_evidence_json(candidate, evidence, artifact, benchmark, evidence_source),
    )
}

fn persist_registry_entry(
    candidate: &ModelManifestEntry,
    promotion: Option<&PromotionEvidence>,
) -> Result<(), AppError> {
    model_artifact::write_registry_entry(candidate.id, &registry_entry_json(candidate, promotion))
}

fn registry_summary() -> String {
    let selected_id = read_default_selection().ok().map(|value| value.model_id);
    match read_registry_entries() {
        Ok(entries) if entries.is_empty() => format!(
            "model registry\n- installed models: 0\n- registry dir: {}",
            model_artifact::paths().registry_dir.display()
        ),
        Ok(entries) => {
            let rows = entries
                .iter()
                .map(|entry| {
                    format!(
                        "- {}{} | status: {} | evidence: {} | sha256: {} | path: {}",
                        entry.id,
                        if selected_id.as_deref() == Some(entry.id.as_str()) {
                            " | default"
                        } else {
                            ""
                        },
                        entry.status,
                        entry.evidence_status,
                        entry.artifact_sha256,
                        entry.artifact_path
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "model registry\n- installed models: {}\n- registry dir: {}\n{}",
                entries.len(),
                model_artifact::paths().registry_dir.display(),
                rows
            )
        }
        Err(err) => format!(
            "model registry\n- 상태: registry 읽기 실패\n- 이유: {}\n- registry dir: {}",
            err.message,
            model_artifact::paths().registry_dir.display()
        ),
    }
}

fn validate_registry_manifest_binding(
    entry: &RegistryEntry,
    candidate: &ModelManifestEntry,
    artifact: ModelArtifactDescriptor,
) -> Result<(), AppError> {
    if entry.display_name != candidate.display_name
        || entry.upstream_model != candidate.upstream_model
        || entry.upstream_url != candidate.upstream_url
        || entry.license_source != candidate.license.source
        || entry.license_checked_at != candidate.license.checked_at
    {
        return Err(AppError::blocked(
            "model registry source/license provenance가 source-backed manifest와 다릅니다.",
        ));
    }
    if Path::new(&entry.artifact_path) != model_artifact_path(artifact) {
        return Err(AppError::blocked(
            "model registry artifact path가 source-backed manifest와 다릅니다.",
        ));
    }
    if entry.artifact_sha256 != artifact.sha256 {
        return Err(AppError::blocked(
            "model registry artifact SHA-256이 source-backed manifest와 다릅니다.",
        ));
    }
    Ok(())
}

fn validate_registry_promotion_binding(
    entry: &RegistryEntry,
    id: &str,
    evidence: Option<&PromotionEvidence>,
) -> Result<(), AppError> {
    if entry.evidence_status != "verified-local-promotion"
        || entry.promotion_evidence_path != promotion_evidence_path(id).display().to_string()
        || evidence.is_none_or(|evidence| {
            entry.backend_version != evidence.backend_version
                || entry.benchmark_run_id != evidence.benchmark_run_id
        })
    {
        return Err(AppError::blocked(
            "model registry promotion binding이 canonical evidence와 다릅니다.",
        ));
    }
    Ok(())
}

fn validated_registry_entry(id: &str) -> Result<RegistryEntry, AppError> {
    let candidate = find_candidate(id)?;
    let entry = read_registry_entries()?
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| {
            AppError::blocked(format!("설치된 model registry entry가 없습니다: {id}"))
        })?;
    if entry.status != "installed" {
        return Err(AppError::blocked(format!(
            "model registry 상태가 installed가 아닙니다: {}",
            entry.status
        )));
    }
    let artifact = source_backed_artifact(candidate)?;
    let expected_path = model_artifact_path(artifact);
    validate_registry_manifest_binding(&entry, candidate, artifact)?;
    let local_state = local_artifact_state(artifact, &expected_path)?;
    if !local_state.verified {
        return Err(AppError::blocked(format!(
            "model registry artifact 재검증 실패: {}",
            local_state.detail
        )));
    }
    if candidate.status != CandidateStatus::Verified {
        let promotion = local_promotion_readiness(candidate)?;
        if !promotion.validation.ready {
            return Err(AppError::blocked(format!(
                "model promotion evidence 재검증 실패:\n- {}",
                promotion.validation.blockers.join("\n- ")
            )));
        }
        validate_registry_promotion_binding(&entry, id, promotion.evidence.as_ref())?;
    }
    Ok(entry)
}

fn default_selection_json(selection: &DefaultSelection) -> String {
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"modelId\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"selectedAtMs\": {}\n}}\n",
        ledger::json_string(&selection.model_id),
        ledger::json_string(&selection.artifact_sha256),
        selection.selected_at_ms
    )
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
            &model_artifact::paths()
                .artifact(candidate.artifact_name.unwrap_or(candidate.id))
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
    let text = model_artifact::read_promotion_evidence(path)?;
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

fn now_ms_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
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
            .any(|blocker| blocker.contains("promotion evidence")));
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
            benchmark_name: benchmark_policy::ADOPTION_BENCHMARK_NAME.to_string(),
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
  "ramFit": "observed-within-local-host",
  "recommendedRamGb": 6,
  "peakRssBytes": 3351363584,
  "mmproj": "not-required-text-only",
  "benchmarkRunId": "benchmark-local",
  "recordedAt": "2026-07-10T00:00:00Z"
}"#;

        let evidence = parse_promotion_evidence(text).unwrap();

        assert_eq!(evidence.model_id, "qwen3.5-4b");
        assert_eq!(evidence.backend_version, "b9878");
        assert_eq!(evidence.recommended_ram_gb, 6);
    }

    #[test]
    fn promotion_evidence_validation_accepts_measured_local_benchmark() {
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();
        let evidence = qwen_promotion_evidence(artifact);
        let benchmark = qwen_benchmark_report(artifact, &evidence);
        let smoke = qwen_backend_smoke(artifact, &evidence);
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
            Some(&smoke),
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
            validate_promotion_evidence(candidate, &evidence, artifact, &local_state, None, None);

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
            .any(|blocker| blocker.contains("smoke event")));
    }

    #[test]
    fn promotion_evidence_rejects_canonical_benchmark_contract_drift() {
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();
        let evidence = qwen_promotion_evidence(artifact);
        let smoke = qwen_backend_smoke(artifact, &evidence);
        let local_state = LocalArtifactState {
            status: "verified-local-artifact",
            detail: "test artifact verified".to_string(),
            verified: true,
        };
        let canonical = qwen_benchmark_report(artifact, &evidence);

        for benchmark in [
            {
                let mut row = canonical.clone();
                row.fixture_sha256 = "a".repeat(64);
                row
            },
            {
                let mut row = canonical.clone();
                row.prompt_artifact_sha256 = Some("b".repeat(64));
                row
            },
            {
                let mut row = canonical.clone();
                row.benchmark_name = "easier-smoke".to_string();
                row
            },
        ] {
            let validation = validate_promotion_evidence(
                candidate,
                &evidence,
                artifact,
                &local_state,
                Some(&benchmark),
                Some(&smoke),
            );
            assert!(!validation.ready);
            assert!(validation
                .blockers
                .iter()
                .any(|blocker| blocker.contains("canonical model adoption smoke")));
        }
    }

    #[test]
    fn registry_parser_accepts_pretty_json_entries() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();
        let text = registry_entry_json(candidate, None);
        let entry = parse_registry_entry(&text).unwrap();

        assert_eq!(entry.id, "qwen3.5-4b");
        assert_eq!(entry.status, "installed");
        assert!(entry.artifact_sha256.starts_with("00fe"));
        validate_registry_manifest_binding(&entry, candidate, artifact).unwrap();

        for drifted in [
            text.replace(candidate.license.source, "https://invalid.example/license"),
            text.replace(candidate.license.checked_at, "1999-01-01"),
            text.replace(candidate.upstream_model, "invalid/model"),
            text.replace(candidate.upstream_url, "https://invalid.example/model"),
        ] {
            let entry = parse_registry_entry(&drifted).unwrap();
            assert!(validate_registry_manifest_binding(&entry, candidate, artifact).is_err());
        }
    }

    #[test]
    fn registry_promotion_binding_rejects_backend_and_benchmark_drift() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let candidate = find_candidate("qwen3.5-4b").unwrap();
        let artifact = source_backed_artifact(candidate).unwrap();
        let evidence = qwen_promotion_evidence(artifact);
        let text = registry_entry_json(candidate, Some(&evidence));
        let entry = parse_registry_entry(&text).unwrap();

        validate_registry_promotion_binding(&entry, candidate.id, Some(&evidence)).unwrap();
        for drifted in [
            text.replace(&evidence.backend_version, "b0000"),
            text.replace(&evidence.benchmark_run_id, "benchmark-drifted"),
        ] {
            let entry = parse_registry_entry(&drifted).unwrap();
            assert!(
                validate_registry_promotion_binding(&entry, candidate.id, Some(&evidence),)
                    .is_err()
            );
        }
    }

    #[test]
    fn default_selection_parser_is_strict_and_round_trips() {
        let selection = DefaultSelection {
            model_id: "qwen3.5-4b".to_string(),
            artifact_sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4"
                .to_string(),
            selected_at_ms: 42,
        };
        assert_eq!(
            parse_default_selection(&default_selection_json(&selection)).unwrap(),
            selection
        );
        assert!(parse_default_selection(
            r#"{"schemaVersion":1,"modelId":"qwen3.5-4b","artifactSha256":"x","selectedAtMs":42,"unknown":true}"#
        )
        .is_err());
    }

    #[test]
    fn default_resolution_fails_closed_when_selection_is_missing() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let data_root =
            std::env::temp_dir().join(format!("rpotato-default-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data_root);
        std::env::set_var("RPOTATO_DATA_HOME", &data_root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", data_root.join("project"));

        let error = default_artifact_path().unwrap_err();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(data_root);
        assert!(error.message.contains("기본 모델이 선택되지 않았습니다"));
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
            ram_fit: "observed-within-local-host".to_string(),
            recommended_ram_gb: measured_ram_budget_gb(3_351_363_584),
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
            model_run_id: Some(format!("model-run-{}", evidence.backend_smoke_event_id)),
            model_id: artifact_model_id(artifact),
            benchmark_name: benchmark_policy::ADOPTION_BENCHMARK_NAME.to_string(),
            fixture_id: benchmark_policy::ADOPTION_FIXTURE_ID.to_string(),
            fixture_sha256: benchmark_policy::ADOPTION_FIXTURE_SHA256.to_string(),
            prompt_artifact_sha256: Some(benchmark_policy::ADOPTION_PROMPT_SHA256.to_string()),
            prompt_chars: Some(147),
            claim_state: "measured-locally".to_string(),
            score: Some(3.0),
            score_unit: Some("0-3-local-product-score".to_string()),
            local_pass: Some(true),
            expected_matches: Some(1),
            expected_total: Some(1),
            forbidden_matches: Some(0),
            harness_ref: "rpotato-benchmark-harness@test".to_string(),
            dataset_ref: Some(benchmark_policy::ADOPTION_DATASET_REF.to_string()),
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

    fn qwen_backend_smoke(
        artifact: ModelArtifactDescriptor,
        evidence: &PromotionEvidence,
    ) -> BackendSmokeEvidence {
        BackendSmokeEvidence {
            event_id: evidence.backend_smoke_event_id.clone(),
            backend_id: "llama.cpp".to_string(),
            backend_release: evidence.backend_version.clone(),
            binary_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            model_id: artifact_model_id(artifact),
            model_sha256: artifact.sha256.to_string(),
            model_size_bytes: artifact.size_bytes,
            ctx_size: "4096".to_string(),
            mmproj: evidence.mmproj.clone(),
            sampling: "temperature-0.1_top-p-0.8".to_string(),
            host_os: "macos".to_string(),
            host_arch: "aarch64".to_string(),
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
