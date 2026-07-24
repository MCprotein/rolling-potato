//! Source-backed model choices and preparation for interactive setup.

use std::path::PathBuf;

use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::find_candidate;
use crate::surfaces::tui::runtime_bridge::TuiModelOption;

use super::fetch_candidate_for_evaluation_report;
use super::registry::{configured_model_id, prepare_user_selected_candidate, set_default_report};

mod catalog;
mod runtime_spec;
#[cfg(test)]
mod tests;
pub(crate) use runtime_spec::{configured_runtime_spec, configured_vision_runtime_spec};

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
        vision_ready: false,
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
