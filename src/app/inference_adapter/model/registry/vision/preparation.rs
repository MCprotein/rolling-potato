//! Lazy projector preparation and atomic legacy-registry binding.

use std::path::Path;

use super::{verified_vision_projector_binding, VerifiedVisionProjector};
use crate::adapters::filesystem::model_artifact::{
    fetch_managed_projector_artifact, local_artifact_state, model_artifact_path,
    read_registry_entries, vision_projector_artifact_path, vision_projector_part_path,
    write_registry_entry,
};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::codec::render_registry_entry_snapshot;
use crate::runtime_core::inference::model::manifest::{
    source_backed_artifact, source_backed_vision_projector, ModelArtifactDescriptor,
    ModelManifestEntry,
};

#[cfg(test)]
mod tests;

pub(crate) fn prepare_bound_vision_projector(
    candidate: &'static ModelManifestEntry,
) -> Result<VerifiedVisionProjector, AppError> {
    let artifact = source_backed_artifact(candidate)?;
    let expected_model_path = model_artifact_path(artifact);
    let projector = source_backed_vision_projector(candidate).ok_or_else(|| {
        AppError::blocked(format!(
            "선택한 모델은 이미지 입력을 지원하는 projector가 없습니다: {}",
            candidate.id
        ))
    })?;
    let path = vision_projector_artifact_path(candidate, projector);
    let part_path = vision_projector_part_path(candidate, projector);
    prepare_bound_vision_projector_artifacts(
        candidate.id,
        artifact,
        &expected_model_path,
        projector,
        &path,
        &part_path,
    )
}

fn prepare_bound_vision_projector_artifacts(
    model_id: &str,
    artifact: ModelArtifactDescriptor,
    expected_model_path: &Path,
    projector: ModelArtifactDescriptor,
    path: &Path,
    part_path: &Path,
) -> Result<VerifiedVisionProjector, AppError> {
    let model_state = local_artifact_state(artifact, &expected_model_path)?;
    if !model_state.verified {
        return Err(AppError::blocked(format!(
            "이미지 기능 준비를 중단했습니다.\n- model: {}\n- 이유: 기존 model artifact 검증 실패 ({})\n- 동작: 기본 모델과 text-ready backend는 변경하지 않습니다.",
            model_id, model_state.detail
        )));
    }
    let mut entry = read_registry_entries()?
        .into_iter()
        .find(|entry| entry.id == model_id)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "이미지 기능을 연결할 model registry entry가 없습니다: {}",
                model_id
            ))
        })?;
    if entry.status != "installed"
        || entry.artifact_sha256 != artifact.sha256
        || Path::new(&entry.artifact_path) != expected_model_path
    {
        return Err(AppError::blocked(format!(
            "이미지 기능 준비를 중단했습니다.\n- model: {}\n- 이유: model registry binding 불일치\n- 동작: 기본 모델과 text-ready backend는 변경하지 않습니다.",
            model_id
        )));
    }
    fetch_managed_projector_artifact(projector, &path, &part_path).map_err(|error| AppError {
        code: error.code,
        message: format!(
            "이미지 기능 준비에 실패했습니다.\n- model: {}\n- projector: {}\n- 이유: {}\n- 동작: 기본 모델과 text-ready backend는 그대로 유지하며 다음 이미지 요청에서 partial download를 이어받습니다.",
            model_id,
            path.display(),
            error.message.replace('\n', " | ")
        ),
    })?;
    entry.vision_status = "ready".to_string();
    entry.mmproj_path = Some(path.display().to_string());
    entry.mmproj_sha256 = Some(projector.sha256.to_string());
    entry.mmproj_size_bytes = Some(projector.size_bytes);
    write_registry_entry(model_id, &render_registry_entry_snapshot(&entry))?;
    verified_vision_projector_binding(
        model_id,
        expected_model_path,
        artifact.sha256,
        projector,
        path.to_path_buf(),
    )
    .ok_or_else(|| {
        AppError::blocked(format!(
            "이미지 projector를 준비했지만 registry 재검증에 실패했습니다: {model_id}"
        ))
    })
}
