use std::fs;
use std::path::PathBuf;

use super::super::{atomic_write, validate_clean_targets, Change, InstallPaths};
use crate::foundation::error::AppError;

const INSTALL_OWNER_FILE: &str = ".rpotato-install-owned";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BinaryRemovalResult {
    pub(crate) change: Change,
    pub(crate) deferred_until_exit: bool,
}

pub(crate) fn validate_clean_uninstall_targets(paths: &InstallPaths) -> Result<(), AppError> {
    validate_clean_targets(paths)?;
    if paths.installed_binary.parent() != Some(paths.user_bin.as_path()) {
        return Err(AppError::blocked(format!(
            "clean uninstall binary 경계가 유효하지 않습니다: {}",
            paths.installed_binary.display()
        )));
    }
    let expected_name = if cfg!(windows) {
        "rpotato.exe"
    } else {
        "rpotato"
    };
    if paths
        .installed_binary
        .file_name()
        .and_then(|name| name.to_str())
        != Some(expected_name)
    {
        return Err(AppError::blocked(format!(
            "clean uninstall binary 이름이 유효하지 않습니다: {}",
            paths.installed_binary.display()
        )));
    }
    Ok(())
}

pub(crate) fn binary_removal_plan(paths: &InstallPaths) -> Result<Change, AppError> {
    validate_clean_uninstall_targets(paths)?;
    if !install_is_owned(paths)? {
        return Ok(Change::Unchanged);
    }
    match fs::symlink_metadata(&paths.installed_binary) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            Ok(Change::Removed)
        }
        Ok(_) => Err(AppError::blocked(format!(
            "clean uninstall binary target이 regular file이 아닙니다: {}",
            paths.installed_binary.display()
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Change::Unchanged),
        Err(err) => Err(AppError::runtime(format!(
            "clean uninstall binary 상태 확인 실패: {} ({err})",
            paths.installed_binary.display()
        ))),
    }
}

pub(crate) fn remove_installed_binary(
    paths: &InstallPaths,
) -> Result<BinaryRemovalResult, AppError> {
    super::super::ensure_no_pending_binary_mutation(paths)?;
    let change = binary_removal_plan(paths)?;
    if change == Change::Unchanged {
        if install_is_owned(paths)? && !paths.installed_binary.exists() {
            remove_install_ownership(paths)?;
        }
        return Ok(BinaryRemovalResult {
            change,
            deferred_until_exit: false,
        });
    }

    #[cfg(windows)]
    if super::super::current_invocation_is_installed(paths) {
        super::windows_cleanup::schedule_windows_self_delete(paths)?;
        remove_install_ownership(paths)?;
        return Ok(BinaryRemovalResult {
            change,
            deferred_until_exit: true,
        });
    }

    fs::remove_file(&paths.installed_binary).map_err(|err| {
        AppError::runtime(format!(
            "clean uninstall binary 삭제 실패: {} ({err})",
            paths.installed_binary.display()
        ))
    })?;
    remove_install_ownership(paths)?;
    #[cfg(windows)]
    super::windows_cleanup::remove_empty_windows_install_dirs(&paths.user_bin)?;
    Ok(BinaryRemovalResult {
        change,
        deferred_until_exit: false,
    })
}

pub(in crate::adapters::system_install) fn record_install_ownership(
    paths: &InstallPaths,
) -> Result<(), AppError> {
    if install_is_owned(paths)? {
        return Ok(());
    }
    atomic_write::atomic_replace_bytes(
        &install_owner_file(paths),
        b"rpotato-owned-user-install-v1\n",
    )
}

pub(in crate::adapters::system_install) fn install_owner_file(paths: &InstallPaths) -> PathBuf {
    paths.user_bin.join(INSTALL_OWNER_FILE)
}

pub(in crate::adapters::system_install) fn install_is_owned(
    paths: &InstallPaths,
) -> Result<bool, AppError> {
    let marker = install_owner_file(paths);
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(AppError::blocked(format!(
            "설치 ownership marker 유형이 유효하지 않습니다: {}",
            marker.display()
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(AppError::runtime(format!(
            "설치 ownership marker 확인 실패: {} ({err})",
            marker.display()
        ))),
    }
}

fn remove_install_ownership(paths: &InstallPaths) -> Result<(), AppError> {
    let marker = install_owner_file(paths);
    match fs::remove_file(&marker) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::runtime(format!(
            "설치 ownership marker 삭제 실패: {} ({err})",
            marker.display()
        ))),
    }
}
