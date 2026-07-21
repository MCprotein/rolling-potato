//! Exact snapshot and rollback of the configured default model selection.

use std::fs;

use crate::adapters::filesystem::model_artifact;
use crate::foundation::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefaultSelectionSnapshot {
    body: Option<Vec<u8>>,
}

pub(crate) fn snapshot_default_selection() -> Result<DefaultSelectionSnapshot, AppError> {
    let path = model_artifact::paths().default_file;
    if !path.exists() {
        return Ok(DefaultSelectionSnapshot { body: None });
    }
    model_artifact::read_default_selection()?;
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
