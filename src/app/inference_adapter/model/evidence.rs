use std::path::Path;

use crate::adapters::filesystem::model_artifact::{
    self, local_artifact_state, model_artifact_path, promotion_evidence_path,
};
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::codec::{
    parse_promotion_evidence, render_promotion_evidence,
};
use crate::runtime_core::inference::model::manifest::{
    source_backed_artifact, source_backed_artifact_blockers, BackendSmokeEvidence,
    InstallValidation, ModelArtifactDescriptor, ModelManifestEntry, PromotionEvidence,
    PromotionReadiness,
};
use crate::runtime_core::inference::model::promotion::{
    artifact_model_id, validate_promotion_evidence, PromotionBenchmarkEvidence,
};

pub(super) fn local_benchmark_status(
    artifact: ModelArtifactDescriptor,
) -> Result<String, AppError> {
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

pub(super) fn local_promotion_readiness(
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

    Ok(PromotionReadiness {
        validation,
        evidence: Some(evidence),
    })
}

pub(super) fn promotion_benchmark_run(
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

pub(super) fn promotion_benchmark_evidence(
    row: &observability::BenchmarkRunReport,
) -> PromotionBenchmarkEvidence {
    PromotionBenchmarkEvidence {
        claim_state: row.claim_state.clone(),
        local_pass: row.local_pass,
        backend_id: row.backend_id.clone(),
        fixture_id: row.fixture_id.clone(),
        fixture_sha256: row.fixture_sha256.clone(),
        prompt_artifact_sha256: row.prompt_artifact_sha256.clone(),
        benchmark_name: row.benchmark_name.clone(),
        score: row.score,
        dataset_ref: row.dataset_ref.clone(),
        peak_rss_bytes: row.peak_rss_bytes,
        model_run_id: row.model_run_id.clone(),
    }
}

pub(super) fn backend_smoke_evidence(
    event_id: &str,
) -> Result<Option<BackendSmokeEvidence>, AppError> {
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

fn detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details.split_whitespace().find_map(|field| {
        let (candidate, value) = field.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

pub(super) fn persist_promotion_evidence(
    candidate: &ModelManifestEntry,
    evidence: &PromotionEvidence,
    artifact: ModelArtifactDescriptor,
    benchmark: &PromotionBenchmarkEvidence,
    evidence_source: &Path,
) -> Result<(), AppError> {
    let artifact_path = model_artifact_path(artifact);
    model_artifact::write_promotion_evidence(
        candidate.id,
        &render_promotion_evidence(
            candidate,
            evidence,
            &artifact_path,
            benchmark,
            evidence_source,
        ),
    )
}

pub(super) fn read_promotion_evidence_file(path: &Path) -> Result<PromotionEvidence, AppError> {
    let text = model_artifact::read_promotion_evidence(path)?;
    parse_promotion_evidence(&text)
}
