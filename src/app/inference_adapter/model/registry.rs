use std::fs;
use std::path::PathBuf;

use super::local_promotion_readiness;
use crate::adapters::filesystem::model_artifact::{
    self, local_artifact_state, model_artifact_path, promotion_evidence_path,
    read_default_selection, read_registry_entries, registry_path,
};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::codec::{
    render_default_selection, render_registry_entry,
};
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, validate_install_ready, CandidateStatus,
    DefaultSelection, ModelManifestEntry, PromotionEvidence, RegistryEntry,
};
use crate::runtime_core::inference::model::promotion::{
    validate_registry_manifest_binding, validate_registry_promotion_binding,
};

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
    let body = render_default_selection(&selection);
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &model_artifact::paths().default_file,
        body.as_bytes(),
    )?;
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

pub(crate) fn configured_model_id() -> Option<String> {
    read_default_selection()
        .ok()
        .map(|selection| selection.model_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefaultSelectionSnapshot {
    body: Option<Vec<u8>>,
}

pub(crate) fn snapshot_default_selection() -> Result<DefaultSelectionSnapshot, AppError> {
    let path = model_artifact::paths().default_file;
    if !path.exists() {
        return Ok(DefaultSelectionSnapshot { body: None });
    }
    read_default_selection()?;
    let body = fs::read(&path).map_err(|err| {
        AppError::runtime(format!(
            "기본 모델 선택 snapshot을 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    Ok(DefaultSelectionSnapshot { body: Some(body) })
}

pub(crate) fn restore_default_selection(
    snapshot: &DefaultSelectionSnapshot,
) -> Result<(), AppError> {
    let path = model_artifact::paths().default_file;
    match &snapshot.body {
        Some(body) => crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, body),
        None if path.exists() => fs::remove_file(&path).map_err(|err| {
            AppError::runtime(format!(
                "실패한 모델 선택을 제거하지 못했습니다: {} ({err})",
                path.display()
            ))
        }),
        None => Ok(()),
    }
}

pub(crate) fn prepare_user_selected_candidate(
    candidate: &'static ModelManifestEntry,
) -> Result<PathBuf, AppError> {
    let artifact = source_backed_artifact(candidate)?;
    let artifact_path = model_artifact_path(artifact);
    let local = local_artifact_state(artifact, &artifact_path)?;
    if !local.verified {
        return Err(AppError::blocked(format!(
            "선택한 모델 artifact 검증 실패\n- id: {}\n- 상태: {}\n- 이유: {}",
            candidate.id, local.status, local.detail
        )));
    }
    if candidate.license.status != "confirmed" || candidate.backend_compatibility.is_none() {
        return Err(AppError::blocked(format!(
            "선택한 모델의 source-backed license/backend 정보가 충분하지 않습니다: {}",
            candidate.id
        )));
    }

    persist_registry_entry(candidate, None)?;
    Ok(artifact_path)
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

pub(super) fn install_ready_for_report(candidate: &'static ModelManifestEntry) -> bool {
    validate_install_ready(candidate).ready
        || local_promotion_readiness(candidate)
            .map(|readiness| readiness.validation.ready)
            .unwrap_or(false)
}

fn persist_registry_entry(
    candidate: &ModelManifestEntry,
    promotion: Option<&PromotionEvidence>,
) -> Result<(), AppError> {
    model_artifact::write_registry_entry(candidate.id, &registry_entry_json(candidate, promotion))
}

pub(super) fn registry_summary() -> String {
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
    validate_registry_manifest_binding(&entry, candidate, artifact, &expected_path)?;
    let local_state = local_artifact_state(artifact, &expected_path)?;
    if !local_state.verified {
        return Err(AppError::blocked(format!(
            "model registry artifact 재검증 실패: {}",
            local_state.detail
        )));
    }
    if candidate.status != CandidateStatus::Verified
        && entry.evidence_status != "source-backed-manifest"
    {
        let promotion = local_promotion_readiness(candidate)?;
        if !promotion.validation.ready {
            return Err(AppError::blocked(format!(
                "model promotion evidence 재검증 실패:\n- {}",
                promotion.validation.blockers.join("\n- ")
            )));
        }
        validate_registry_promotion_binding(
            &entry,
            &promotion_evidence_path(id),
            promotion.evidence.as_ref(),
        )?;
    }
    if candidate.status != CandidateStatus::Verified
        && entry.evidence_status == "source-backed-manifest"
        && (candidate.license.status != "confirmed" || candidate.backend_compatibility.is_none())
    {
        return Err(AppError::blocked(
            "사용자 선택 model의 source-backed license/backend 정보가 유효하지 않습니다.",
        ));
    }
    Ok(entry)
}

pub(super) fn registry_entry_json(
    candidate: &ModelManifestEntry,
    promotion: Option<&PromotionEvidence>,
) -> String {
    let artifact_path =
        model_artifact::paths().artifact(candidate.artifact_name.unwrap_or(candidate.id));
    let evidence_path = promotion.map(|_| promotion_evidence_path(candidate.id));
    render_registry_entry(
        candidate,
        promotion,
        &artifact_path,
        evidence_path.as_deref(),
    )
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
