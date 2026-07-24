//! Validated default-model runtime specification.

use std::path::PathBuf;

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
        || PathBuf::from(&entry.artifact_path) != expected_model_path
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
    let vision_projector_path = match source_backed_vision_projector(candidate) {
        Some(projector) => {
            let expected = vision_projector_artifact_path(candidate, projector);
            if entry.vision_status != "ready"
                || entry.mmproj_path.as_deref().map(PathBuf::from) != Some(expected.clone())
                || entry.mmproj_sha256.as_deref() != Some(projector.sha256)
                || entry.mmproj_size_bytes != Some(projector.size_bytes)
            {
                return Err(AppError::blocked(format!(
                    "기본 모델의 vision projector registry binding이 준비되지 않았습니다: {}",
                    selection.model_id
                )));
            }
            Some(expected)
        }
        None => None,
    };
    Ok(ConfiguredRuntimeSpec {
        model_id: selection.model_id,
        model_path: expected_model_path,
        context_tokens,
        vision_projector_path,
    })
}
