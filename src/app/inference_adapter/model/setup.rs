//! Source-backed model choices and preparation for interactive setup.

use std::path::PathBuf;

use crate::adapters::filesystem::model_artifact::{
    local_artifact_state, vision_projector_artifact_path,
};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_vision_projector,
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
