//! Source-backed model choices and preparation for interactive setup.

use std::path::PathBuf;

use crate::adapters::filesystem::model_artifact::{
    local_artifact_state, vision_projector_artifact_path,
};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, source_backed_vision_projector, CANDIDATES,
};
use crate::surfaces::tui::runtime_bridge::TuiModelOption;

use super::fetch_candidate_for_evaluation_report;
use super::registry::{configured_model_id, prepare_user_selected_candidate, set_default_report};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedSetupModel {
    pub(crate) id: String,
    pub(crate) artifact_path: PathBuf,
    pub(crate) context_tokens: u32,
    pub(crate) vision_ready: bool,
}

pub(crate) fn setup_options() -> Vec<TuiModelOption> {
    let current = configured_model_id();
    CANDIDATES
        .iter()
        .filter(|candidate| source_backed_artifact(candidate).is_ok())
        .map(|candidate| TuiModelOption {
            id: candidate.id.to_string(),
            display_name: candidate.display_name.to_string(),
            quantization: candidate.quantization.unwrap_or("미확정").to_string(),
            download_bytes: candidate.size_bytes.unwrap_or(0).saturating_add(
                source_backed_vision_projector(candidate)
                    .map(|artifact| artifact.size_bytes)
                    .unwrap_or(0),
            ),
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
                "vision 지원(mmproj 자동 준비); 로컬 adoption smoke 통과; 16 GB 적합성은 미확정"
                    .to_string()
            } else {
                "vision 지원(mmproj 자동 준비); 실험적 선택; exact-response adoption gate 미통과"
                    .to_string()
            },
            current: current.as_deref() == Some(candidate.id),
            recommended: candidate.id == "gemma-4-e4b",
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_declared_projector_blocks_setup_before_model_switch() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-setup-projector-test-{}",
            std::process::id()
        ));
        let previous = std::env::var_os("RPOTATO_DATA_HOME");
        let _ = std::fs::remove_dir_all(&root);
        std::env::set_var("RPOTATO_DATA_HOME", &root);
        let candidate = find_candidate("gemma-4-e4b").unwrap();

        let error = require_declared_projector(candidate).unwrap_err();

        if let Some(previous) = previous {
            std::env::set_var("RPOTATO_DATA_HOME", previous);
        } else {
            std::env::remove_var("RPOTATO_DATA_HOME");
        }
        let _ = std::fs::remove_dir_all(root);
        assert!(error.message.contains("모델 변경을 중단"));
        assert!(error.message.contains("현재 모델과 backend는 그대로 유지"));
    }

    #[test]
    fn setup_options_expose_each_models_manifest_context_limit() {
        let options = setup_options();

        assert_eq!(
            options
                .iter()
                .find(|option| option.id == "qwen3.5-4b")
                .and_then(|option| option.context_length),
            Some(262_144)
        );
        assert_eq!(
            options
                .iter()
                .find(|option| option.id == "gemma-4-e4b")
                .and_then(|option| option.context_length),
            Some(131_072)
        );
    }
}
