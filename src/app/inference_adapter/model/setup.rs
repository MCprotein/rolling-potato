//! Source-backed model choices and preparation for interactive setup.

use std::path::PathBuf;

use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, CANDIDATES,
};
use crate::surfaces::tui::runtime_bridge::TuiModelOption;

use super::fetch_candidate_for_evaluation_report;
use super::registry::{prepare_user_selected_candidate, set_default_report};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedSetupModel {
    pub(crate) id: String,
    pub(crate) artifact_path: PathBuf,
}

pub(crate) fn setup_options() -> Vec<TuiModelOption> {
    CANDIDATES
        .iter()
        .filter(|candidate| source_backed_artifact(candidate).is_ok())
        .map(|candidate| TuiModelOption {
            id: candidate.id.to_string(),
            display_name: candidate.display_name.to_string(),
            quantization: candidate.quantization.unwrap_or("미확정").to_string(),
            download_bytes: candidate.size_bytes.unwrap_or(0),
            context_length: candidate.context_length,
            ram: candidate
                .recommended_ram_gb
                .map(|value| format!("{value} GiB"))
                .unwrap_or_else(|| "미확정".to_string()),
            license: if candidate
                .license
                .claim
                .to_ascii_lowercase()
                .contains("apache-2.0")
            {
                "Apache-2.0".to_string()
            } else {
                candidate.license.status.to_string()
            },
            note: if candidate.id == "gemma-4-e4b" {
                "로컬 adoption smoke 통과; 16 GB 적합성은 미확정".to_string()
            } else {
                "실험적 선택; exact-response adoption gate 미통과".to_string()
            },
            recommended: candidate.id == "gemma-4-e4b",
        })
        .collect()
}

pub(crate) fn prepare_setup_model(id: &str) -> Result<PreparedSetupModel, AppError> {
    let candidate = find_candidate(id)?;
    fetch_candidate_for_evaluation_report(id)?;
    let artifact_path = prepare_user_selected_candidate(candidate)?;
    Ok(PreparedSetupModel {
        id: id.to_string(),
        artifact_path,
    })
}

pub(crate) fn activate_setup_model(id: &str) -> Result<(), AppError> {
    set_default_report(id).map(|_| ())
}
