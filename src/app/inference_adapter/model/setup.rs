//! Source-backed model choices and preparation for interactive setup.

use std::path::PathBuf;

use crate::adapters::filesystem::model_artifact::{
    local_artifact_state, model_artifact_path, read_default_selection, read_registry_entries,
    vision_projector_artifact_path,
};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, source_backed_vision_projector,
};
use crate::surfaces::tui::runtime_bridge::TuiModelOption;

use super::fetch_candidate_for_evaluation_report;
use super::registry::{configured_model_id, prepare_user_selected_candidate, set_default_report};

mod catalog;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedSetupModel {
    pub(crate) id: String,
    pub(crate) artifact_path: PathBuf,
    pub(crate) context_tokens: u32,
    pub(crate) vision_ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfiguredRuntimeSpec {
    pub(crate) model_id: String,
    pub(crate) model_path: PathBuf,
    pub(crate) context_tokens: u32,
    pub(crate) vision_projector_path: Option<PathBuf>,
}

pub(crate) fn setup_options() -> Vec<TuiModelOption> {
    catalog::setup_options()
}

pub(crate) fn prepare_setup_model(id: &str) -> Result<PreparedSetupModel, AppError> {
    let candidate = find_candidate(id)?;
    fetch_candidate_for_evaluation_report(id)?;
    let vision_ready = require_declared_projector(candidate)?;
    let artifact_path = prepare_user_selected_candidate(candidate)?;
    let context_tokens = candidate
        .context_length
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "선택한 모델의 최대 context length가 manifest에 없습니다: {id}"
            ))
        })?;
    Ok(PreparedSetupModel {
        id: id.to_string(),
        artifact_path,
        context_tokens,
        vision_ready,
    })
}

pub(crate) fn activate_setup_model(id: &str) -> Result<(), AppError> {
    set_default_report(id).map(|_| ())
}

pub(crate) fn configured_context_length() -> Result<u32, AppError> {
    let id = configured_model_id()
        .ok_or_else(|| AppError::blocked("기본 모델이 선택되지 않았습니다."))?;
    find_candidate(&id)?
        .context_length
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "기본 모델의 최대 context length가 manifest에 없습니다: {id}"
            ))
        })
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

fn require_declared_projector(
    candidate: &crate::runtime_core::inference::model::manifest::ModelManifestEntry,
) -> Result<bool, AppError> {
    let Some(projector) = source_backed_vision_projector(candidate) else {
        return Ok(false);
    };
    let path = vision_projector_artifact_path(candidate, projector);
    let state = local_artifact_state(projector, &path)?;
    if state.verified {
        return Ok(true);
    }
    Err(AppError::blocked(format!(
        "vision projector 준비에 실패해 모델 변경을 중단했습니다.\n- model: {}\n- projector: {}\n- 상태: {}\n- 이유: {}\n- 동작: 현재 모델과 backend는 그대로 유지하며, 다음 선택 시 partial download를 이어받습니다.",
        candidate.id,
        path.display(),
        state.status,
        state.detail
    )))
}
