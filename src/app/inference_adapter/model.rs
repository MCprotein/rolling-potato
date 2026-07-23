use std::path::PathBuf;

use crate::adapters::filesystem::model_artifact::{
    self, fetch_evaluation_artifact, local_artifact_state, model_artifact_part_path,
    model_artifact_path, promotion_evidence_path, vision_projector_artifact_path,
    vision_projector_part_path,
};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, source_backed_artifact_blockers,
    source_backed_vision_projector, validate_install_ready, ManifestCounts, CANDIDATES,
    STATUS_SCHEMA,
};
use crate::runtime_core::inference::model::promotion::validate_promotion_evidence;

mod evidence;
mod registry;
mod setup;

use evidence::{
    backend_smoke_evidence, local_benchmark_status, local_promotion_readiness,
    persist_promotion_evidence, promotion_benchmark_evidence, promotion_benchmark_run,
    read_promotion_evidence_file,
};

#[cfg(test)]
use registry::registry_entry_json;
pub(crate) use registry::{
    configured_model_id, restore_default_selection, snapshot_default_selection,
    verified_vision_projector, DefaultSelectionSnapshot,
};
pub use registry::{
    default_artifact_path, default_report, install_candidate, registry_report, set_default_report,
};
use registry::{install_ready_for_report, registry_summary};
pub(crate) use setup::{activate_setup_model, prepare_setup_model, setup_options};

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
    let projector_status = source_backed_vision_projector(candidate).map(|projector| {
        let projector_path = vision_projector_artifact_path(candidate, projector);
        let projector_part_path = vision_projector_part_path(candidate, projector);
        match fetch_evaluation_artifact(projector, &projector_path, &projector_part_path) {
            Ok(status) => format!(
                "ready ({}, {}, sha256 {})",
                status.label(),
                projector_path.display(),
                projector.sha256
            ),
            Err(error) => format!(
                "unavailable (텍스트 모델은 유지됨; projector 준비 실패: {})",
                error.message.replace('\n', " | ")
            ),
        }
    });
    let event_id = state::record_event(
        "model.evaluation_artifact.fetched",
        "검증용 model artifact fetch 완료",
        &format!(
            "model_id={} provider={} artifact={} sha256={} size_bytes={} status={} vision_status={} registry=not_registered",
            candidate.id,
            artifact.provider,
            final_path.display(),
            artifact.sha256,
            artifact.size_bytes,
            fetch_status.label(),
            projector_status.as_deref().unwrap_or("not-declared")
        ),
    )?;

    Ok(format!(
        "검증용 model artifact 준비 완료\n- id: {}\n- text status: {}\n- vision status: {}\n- provider: {}\n- source: {}\n- terms: {}\n- file: {}\n- size bytes: {}\n- sha256: {}\n- partial path: {}\n- final path: {}\n- registry: not registered\n- ledger event: {}\n- 동작: mmproj 준비가 실패해도 검증된 text artifact와 현재 선택은 유지합니다.\n- 다음 단계: rpotato backend start --model {} --ctx-size 4096 으로 local smoke를 실행하고, benchmark/RAM-fit/mmproj evidence가 쌓인 뒤에만 verified 승격을 검토합니다.",
        candidate.id,
        fetch_status.label(),
        projector_status.as_deref().unwrap_or("not-declared"),
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
    let benchmark_evidence = benchmark.as_ref().map(promotion_benchmark_evidence);
    let backend_smoke = backend_smoke_evidence(&evidence.backend_smoke_event_id)?;
    let validation = validate_promotion_evidence(
        candidate,
        &evidence,
        artifact,
        &local_state,
        benchmark_evidence.as_ref(),
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
    let benchmark_evidence = promotion_benchmark_evidence(&benchmark);
    persist_promotion_evidence(
        candidate,
        &evidence,
        artifact,
        &benchmark_evidence,
        &evidence_source,
    )?;
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
    let actual_sha256 = model_artifact::sha256_for_file(&path)?;
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
    let cleanup = model_artifact::cleanup_failed_artifacts(candidate, dry_run)?;

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
            candidate.id, dry_run, cleanup.removed, cleanup.missing
        ),
    )?;

    Ok(format!(
        "failed artifact cleanup {}\n- id: {}\n- removed: {}\n- missing: {}\n- ledger event: {}\n{}\n- boundary: app data downloads/models 아래의 failed/partial artifact만 대상으로 합니다.",
        if dry_run { "dry-run" } else { "결과" },
        candidate.id,
        cleanup.removed,
        cleanup.missing,
        event_id,
        cleanup.rows.join("\n")
    ))
}

fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;
