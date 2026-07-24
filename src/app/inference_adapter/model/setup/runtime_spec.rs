//! Validated default-model runtime specification.

use std::path::{Path, PathBuf};

use crate::adapters::filesystem::model_artifact::{
    model_artifact_path, read_default_selection, read_registry_entries,
    vision_projector_artifact_path,
};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, source_backed_vision_projector,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfiguredRuntimeSpec {
    pub(crate) model_id: String,
    pub(crate) model_path: PathBuf,
    pub(crate) context_tokens: u32,
    pub(crate) vision_projector_path: Option<PathBuf>,
}

pub(crate) fn configured_runtime_spec() -> Result<ConfiguredRuntimeSpec, AppError> {
    let selection = read_default_selection()?;
    let candidate = find_candidate(&selection.model_id)?;
    let artifact = source_backed_artifact(candidate)?;
    let expected_model_path = model_artifact_path(artifact);
    let entry = read_registry_entries()?
        .into_iter()
        .find(|entry| entry.id == selection.model_id)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "기본 모델의 registry entry가 없습니다: {}",
                selection.model_id
            ))
        })?;
    if entry.status != "installed"
        || entry.artifact_sha256 != selection.artifact_sha256
        || entry.artifact_sha256 != artifact.sha256
        || Path::new(&entry.artifact_path) != expected_model_path.as_path()
    {
        return Err(AppError::blocked(format!(
            "기본 모델의 selection·manifest·registry binding이 일치하지 않습니다: {}",
            selection.model_id
        )));
    }
    let context_tokens = candidate
        .context_length
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "기본 모델의 최대 context length가 manifest에 없습니다: {}",
                selection.model_id
            ))
        })?;
    let vision_projector_path = source_backed_vision_projector(candidate).and_then(|projector| {
        let expected = vision_projector_artifact_path(candidate, projector);
        (entry.vision_status == "ready"
            && entry.mmproj_path.as_deref().map(Path::new) == Some(expected.as_path())
            && entry.mmproj_sha256.as_deref() == Some(projector.sha256)
            && entry.mmproj_size_bytes == Some(projector.size_bytes))
        .then_some(expected)
    });
    Ok(ConfiguredRuntimeSpec {
        model_id: selection.model_id,
        model_path: expected_model_path,
        context_tokens,
        vision_projector_path,
    })
}

pub(crate) fn configured_vision_runtime_spec() -> Result<ConfiguredRuntimeSpec, AppError> {
    let mut configured = configured_runtime_spec()?;
    let candidate = find_candidate(&configured.model_id)?;
    let artifact = source_backed_artifact(candidate)?;
    if source_backed_vision_projector(candidate).is_none() {
        return Err(AppError::blocked(format!(
            "선택한 모델은 이미지 입력을 지원하지 않습니다: {}",
            configured.model_id
        )));
    }
    if let Some(projector) = crate::app::inference_adapter::model::verified_vision_projector(
        &configured.model_path,
        artifact.sha256,
    ) {
        configured.vision_projector_path = Some(projector.path);
        return Ok(configured);
    }
    let projector =
        crate::app::inference_adapter::model::prepare_bound_vision_projector(candidate)?;
    configured.vision_projector_path = Some(projector.path);
    Ok(configured)
}
