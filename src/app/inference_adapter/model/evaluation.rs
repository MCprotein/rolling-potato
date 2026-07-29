use crate::adapters::filesystem::model_artifact::{
    fetch_evaluation_artifact, local_artifact_state, model_artifact_part_path, model_artifact_path,
};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, source_backed_artifact_blockers,
    ModelArtifactFetchStatus,
};

use super::evidence::local_benchmark_status;

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

pub fn fetch_candidate_for_evaluation_report(id: &str) -> Result<String, AppError> {
    let candidate = find_candidate(id)?;
    let artifact = source_backed_artifact(candidate)?;
    let final_path = model_artifact_path(artifact);
    let part_path = model_artifact_part_path(candidate);
    let (fetch_status, event_id) = fetch_candidate_for_evaluation(id)?;

    Ok(format!(
        "검증용 model artifact 준비 완료\n- id: {}\n- text status: {}\n- vision status: deferred-until-image-use\n- provider: {}\n- source: {}\n- terms: {}\n- file: {}\n- size bytes: {}\n- sha256: {}\n- partial path: {}\n- final path: {}\n- registry: not registered\n- ledger event: {}\n- 동작: projector는 이미지가 처음 입력될 때만 검증·준비하며 text artifact와 현재 선택을 차단하지 않습니다.\n- 다음 단계: TUI에서 이 모델을 선택하세요. 이미지 첨부 시 필요한 projector를 최초 1회 준비하고 이후 cache를 재사용합니다.",
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
        event_id
    ))
}

pub(crate) fn fetch_candidate_for_evaluation(
    id: &str,
) -> Result<(ModelArtifactFetchStatus, String), AppError> {
    let candidate = find_candidate(id)?;
    let artifact = source_backed_artifact(candidate)?;
    let final_path = model_artifact_path(artifact);
    let part_path = model_artifact_part_path(candidate);
    let fetch_status = fetch_evaluation_artifact(artifact, &final_path, &part_path)?;
    let event_id = state::record_event(
        "model.evaluation_artifact.fetched",
        "검증용 model artifact fetch 완료",
        &format!(
            "model_id={} provider={} artifact={} sha256={} size_bytes={} status={} vision_status=deferred-until-image-use registry=not_registered",
            candidate.id,
            artifact.provider,
            final_path.display(),
            artifact.sha256,
            artifact.size_bytes,
            fetch_status.label()
        ),
    )?;
    Ok((fetch_status, event_id))
}
