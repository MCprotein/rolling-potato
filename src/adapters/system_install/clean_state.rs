//! Guarded removal of managed runtime state.

use std::env;
use std::fs;
use std::path::Path;

use super::path_safety::{absolute_path, paths_resolve_equal, resolve_existing_path};
use super::{CleanStateResult, InstallPaths};
use crate::foundation::error::AppError;

pub(crate) fn validate_clean_targets(paths: &InstallPaths) -> Result<(), AppError> {
    let app_data = absolute_path(&paths.app_data)?;
    let project_root = absolute_path(&paths.project_root)?;
    let project_state = absolute_path(&paths.project_state)?;
    let user_home = absolute_path(&paths.user_home)?;
    let source_binary = absolute_path(&paths.source_binary)?;
    let installed_binary = absolute_path(&paths.installed_binary)?;
    let current_dir = env::current_dir()
        .map_err(|err| AppError::runtime(format!("현재 directory 확인 실패: {err}")))?;
    let resolved_app_data = resolve_existing_path(&app_data);
    let resolved_project_root = resolve_existing_path(&project_root);
    let resolved_project_state = resolve_existing_path(&project_state);
    let resolved_user_home = resolve_existing_path(&user_home);
    let resolved_source_binary = resolve_existing_path(&source_binary);
    let resolved_installed_binary = resolve_existing_path(&installed_binary);
    let resolved_current_dir = resolve_existing_path(&current_dir);

    if project_state.file_name().and_then(|name| name.to_str()) != Some(".rpotato") {
        return Err(AppError::blocked(format!(
            "clean install project-state 경계가 유효하지 않습니다: {}",
            project_state.display()
        )));
    }
    for protected in [&user_home, &project_root] {
        if paths_resolve_equal(&app_data, protected) {
            return Err(AppError::blocked(format!(
                "clean install이 보호 경로를 app-data root로 삭제하려 해 차단했습니다: {}",
                app_data.display()
            )));
        }
    }
    if app_data.parent().is_none()
        || resolved_project_root.starts_with(&resolved_app_data)
        || resolved_user_home.starts_with(&resolved_app_data)
        || resolved_source_binary.starts_with(&resolved_app_data)
        || resolved_installed_binary.starts_with(&resolved_app_data)
        || resolved_current_dir.starts_with(&resolved_app_data)
    {
        return Err(AppError::blocked(format!(
            "clean install app-data 경계가 너무 넓어 차단했습니다: {}",
            app_data.display()
        )));
    }
    if resolved_source_binary.starts_with(&resolved_project_state)
        || resolved_installed_binary.starts_with(&resolved_project_state)
        || resolved_user_home.starts_with(&resolved_project_state)
        || resolved_current_dir.starts_with(&resolved_project_state)
    {
        return Err(AppError::blocked(format!(
            "clean install project-state 안의 보호 경로를 삭제하려 해 차단했습니다: {}",
            project_state.display()
        )));
    }
    Ok(())
}

pub(crate) fn remove_clean_state(paths: &InstallPaths) -> Result<CleanStateResult, AppError> {
    validate_clean_targets(paths)?;
    let app_data_removed = remove_managed_path(&paths.app_data)?;
    let project_state_removed = remove_managed_path(&paths.project_state)?;
    Ok(CleanStateResult {
        app_data_removed,
        project_state_removed,
    })
}

fn remove_managed_path(path: &Path) -> Result<bool, AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "clean install target 상태 확인 실패: {} ({err})",
                path.display()
            )));
        }
    };
    let result = if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
    } else if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        return Err(AppError::blocked(format!(
            "clean install target 유형을 삭제할 수 없습니다: {}",
            path.display()
        )));
    };
    result.map(|_| true).map_err(|err| {
        AppError::runtime(format!(
            "clean install 삭제 실패: {} ({err})",
            path.display()
        ))
    })
}
