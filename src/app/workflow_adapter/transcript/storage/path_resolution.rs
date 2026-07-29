use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;

use super::contract::validate_id;

pub(in super::super) fn validated_tool_output_path(
    project_id: &str,
    session_id: &str,
    workflow_id: &str,
    artifact_id: &str,
    create_parent: bool,
) -> Result<PathBuf, AppError> {
    for (label, value) in [
        ("project id", project_id),
        ("session id", session_id),
        ("workflow id", workflow_id),
        ("tool artifact id", artifact_id),
    ] {
        validate_id(label, value)?;
    }
    let app_root = paths::app_data_root();
    ensure_directory_boundary(&app_root, create_parent, true)?;
    let app_root = fs::canonicalize(&app_root)
        .map_err(|err| AppError::blocked(format!("app-data root 해석 실패: {err}")))?;
    ensure_directory_boundary(&paths::state_dir(), create_parent, false)?;
    let root = paths::tool_outputs_dir();
    ensure_directory_boundary(&root, create_parent, false)?;
    let root_canonical = fs::canonicalize(&root)
        .map_err(|err| AppError::blocked(format!("tool-output root 해석 실패: {err}")))?;
    if !root_canonical.starts_with(&app_root) {
        return Err(AppError::blocked("tool-output app-data 경계 이탈 차단"));
    }
    let mut parent = root;
    for component in [project_id, session_id, workflow_id] {
        parent.push(component);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(AppError::blocked(format!(
                    "tool-output path boundary 불일치: {}",
                    parent.display()
                )))
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound && create_parent => {
                fs::create_dir(&parent).map_err(|err| {
                    AppError::runtime(format!(
                        "tool-output directory 생성 실패: {} ({err})",
                        parent.display()
                    ))
                })?;
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(AppError::blocked(format!(
                    "tool-output directory 누락: {}",
                    parent.display()
                )))
            }
            Err(err) => {
                return Err(AppError::blocked(format!(
                    "tool-output directory 검사 실패: {} ({err})",
                    parent.display()
                )))
            }
        }
    }
    let parent_canonical = fs::canonicalize(&parent)
        .map_err(|err| AppError::blocked(format!("tool-output parent 해석 실패: {err}")))?;
    if !parent_canonical.starts_with(&root_canonical) {
        return Err(AppError::blocked("tool-output root 이탈 차단"));
    }
    let path = paths::tool_output_file(project_id, session_id, workflow_id, artifact_id);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::blocked("tool-output artifact path type 불일치"));
        }
        let canonical = fs::canonicalize(&path)
            .map_err(|err| AppError::blocked(format!("tool-output artifact 해석 실패: {err}")))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(AppError::blocked("tool-output artifact root 이탈 차단"));
        }
    }
    Ok(path)
}

pub(in super::super) fn validated_transcript_path(
    project_id: &str,
    session_id: &str,
    record_id: &str,
    create_parent: bool,
) -> Result<PathBuf, AppError> {
    validate_id("project id", project_id)?;
    validate_id("session id", session_id)?;
    validate_id("record id", record_id)?;

    let app_root = paths::app_data_root();
    ensure_directory_boundary(&app_root, create_parent, true)?;
    let app_root_canonical = fs::canonicalize(&app_root).map_err(|err| {
        AppError::blocked(format!(
            "app-data root 해석 실패: {} ({err})",
            app_root.display()
        ))
    })?;
    let state_root = paths::state_dir();
    ensure_directory_boundary(&state_root, create_parent, false)?;
    let root = paths::transcripts_dir();
    ensure_directory_boundary(&root, create_parent, false)?;
    let root_canonical = fs::canonicalize(&root).map_err(|err| {
        AppError::blocked(format!(
            "transcript root 해석 실패: {} ({err})",
            root.display()
        ))
    })?;
    if !root_canonical.starts_with(&app_root_canonical) {
        return Err(AppError::blocked("transcript root app-data 경계 이탈 차단"));
    }

    let mut parent = root.clone();
    for component in [project_id, session_id] {
        parent.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&parent) {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AppError::blocked(format!(
                    "transcript path boundary 불일치: {}",
                    parent.display()
                )));
            }
        } else if create_parent {
            fs::create_dir(&parent).map_err(|err| {
                AppError::runtime(format!(
                    "transcript directory 생성 실패: {} ({err})",
                    parent.display()
                ))
            })?;
        }
    }
    let parent_canonical = fs::canonicalize(&parent).map_err(|err| {
        AppError::blocked(format!(
            "transcript directory 해석 실패: {} ({err})",
            parent.display()
        ))
    })?;
    if !parent_canonical.starts_with(&root_canonical) {
        return Err(AppError::blocked("transcript path root 이탈 차단"));
    }

    let path = paths::transcript_file(project_id, session_id, record_id);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::blocked(format!(
                "transcript artifact path boundary 불일치: {}",
                path.display()
            )));
        }
        let canonical = fs::canonicalize(&path)
            .map_err(|err| AppError::blocked(format!("transcript artifact 해석 실패: {err}")))?;
        if !canonical.starts_with(&root_canonical) {
            return Err(AppError::blocked("transcript artifact root 이탈 차단"));
        }
    }
    Ok(path)
}

fn ensure_directory_boundary(
    path: &Path,
    create: bool,
    create_ancestors: bool,
) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(AppError::blocked(format!(
                "transcript directory boundary 불일치: {}",
                path.display()
            )));
        }
        Ok(_) => return Ok(()),
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            return Err(AppError::blocked(format!(
                "transcript directory 검사 실패: {} ({err})",
                path.display()
            )));
        }
        Err(_) if !create => {
            return Err(AppError::blocked(format!(
                "transcript directory 누락: {}",
                path.display()
            )));
        }
        Err(_) => {}
    }

    let result = if create_ancestors {
        fs::create_dir_all(path)
    } else {
        fs::create_dir(path)
    };
    result.map_err(|err| {
        AppError::runtime(format!(
            "transcript directory 생성 실패: {} ({err})",
            path.display()
        ))
    })
}
