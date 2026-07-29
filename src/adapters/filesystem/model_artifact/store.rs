//! Registry, evidence, and default-selection persistence.

use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::{atomic_write, layout};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::model::codec::{parse_default_selection, parse_registry_entry};
use crate::runtime_core::inference::model::manifest::{DefaultSelection, RegistryEntry};
use crate::runtime_core::inference::model::ModelArtifactPaths;

pub(crate) fn paths() -> ModelArtifactPaths {
    ModelArtifactPaths {
        downloads_dir: layout::downloads_dir(),
        models_dir: layout::models_dir(),
        registry_dir: layout::model_registry_dir(),
        evidence_dir: layout::model_evidence_dir(),
        default_file: layout::model_default_file(),
        observability_db_file: layout::observability_db_file(),
    }
}

pub(crate) fn registry_path(id: &str) -> PathBuf {
    paths().registry_entry(id)
}

pub(crate) fn promotion_evidence_path(id: &str) -> PathBuf {
    paths().promotion_evidence(id)
}

pub(crate) fn write_registry_entry(id: &str, contents: &str) -> Result<(), AppError> {
    let path = registry_path(id);
    atomic_write::atomic_replace_bytes(&path, contents.as_bytes())
}

pub(crate) fn read_registry_entries() -> Result<Vec<RegistryEntry>, AppError> {
    let dir = paths().registry_dir;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|err| {
        AppError::runtime(format!(
            "model registry directory를 읽지 못했습니다: {} ({err})",
            dir.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "model registry entry를 읽지 못했습니다: {} ({err})",
                dir.display()
            ))
        })?;
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let text = fs::read_to_string(entry.path()).map_err(|err| {
            AppError::runtime(format!(
                "model registry entry를 읽지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;
        entries.push(parse_registry_entry(&text)?);
    }
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(entries)
}

pub(crate) fn write_promotion_evidence(id: &str, contents: &str) -> Result<(), AppError> {
    let path = promotion_evidence_path(id);
    atomic_write::atomic_replace_bytes(&path, contents.as_bytes())
}

pub(crate) fn read_promotion_evidence(path: &Path) -> Result<String, AppError> {
    fs::read_to_string(path).map_err(|err| {
        AppError::runtime(format!(
            "model promotion evidence를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

pub(crate) fn read_default_selection() -> Result<DefaultSelection, AppError> {
    let path = paths().default_file;
    if !path.exists() {
        return Err(AppError::blocked(format!(
            "기본 모델이 선택되지 않았습니다. `rpotato model default <id>`를 실행하세요.\n- selection: {}",
            path.display()
        )));
    }
    let text = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "기본 모델 선택을 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_default_selection(&text)
}
