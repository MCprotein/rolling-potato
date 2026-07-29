use std::path::PathBuf;

use crate::adapters::filesystem::model_artifact::{
    local_artifact_state, model_artifact_path, promotion_evidence_path,
};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::{find_candidate, source_backed_artifact};
use crate::runtime_core::inference::model::promotion::validate_promotion_evidence;

use super::evidence::{
    backend_smoke_evidence, persist_promotion_evidence, promotion_benchmark_evidence,
    promotion_benchmark_run, read_promotion_evidence_file,
};

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
