//! User-local CLI installation facade.

use std::env;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::{atomic_write, layout};
use crate::foundation::error::AppError;

mod binary;
mod clean_state;
mod path_registration;
mod path_safety;
mod uninstall;

pub(crate) use binary::{
    binary_install_plan, ensure_no_pending_binary_mutation, install_binary,
    update_installed_binary, validate_installed_update_target,
};
pub(crate) use clean_state::{remove_clean_state, validate_clean_targets};
pub(crate) use path_registration::{ensure_user_path, user_path_change_plan};
pub(crate) use uninstall::{
    binary_removal_plan, remove_installed_binary, remove_user_path, user_path_removal_plan,
    validate_clean_uninstall_targets,
};

#[cfg(test)]
use binary::{
    apply_staged_update, pending_update_marker_path, reserve_windows_update_marker,
    WINDOWS_SELF_UPDATE_SCRIPT,
};
use path_registration::exact_line_ranges;
#[cfg(test)]
use path_registration::render_managed_profile;
#[cfg(unix)]
use path_registration::resolve_profile_target;
#[cfg(all(test, unix))]
use path_registration::unix_path_plan;
#[cfg(all(test, windows))]
use path_registration::windows_path_registration;
#[cfg(windows)]
use path_registration::{remove_windows_path_ownership, windows_path_is_owned, WindowsPathScope};
#[cfg(test)]
use uninstall::{install_owner_file, render_profile_without_managed_block, BinaryRemovalResult};
#[cfg(all(test, windows))]
use uninstall::{windows_path_owner_file, windows_path_removal};

const PROFILE_BEGIN: &str = "# >>> rpotato managed PATH >>>";
const PROFILE_END: &str = "# <<< rpotato managed PATH <<<";
#[cfg(windows)]
const WINDOWS_PATH_OWNER_FILE: &str = ".rpotato-path-owned";

#[derive(Debug, Clone)]
pub(crate) struct InstallPaths {
    pub(crate) source_binary: PathBuf,
    pub(crate) installed_binary: PathBuf,
    pub(crate) user_bin: PathBuf,
    pub(crate) user_home: PathBuf,
    pub(crate) app_data: PathBuf,
    pub(crate) project_root: PathBuf,
    pub(crate) project_state: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    Created,
    Updated,
    Removed,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryUpdateResult {
    Applied,
    #[cfg(windows)]
    DeferredUntilExit,
}

impl Change {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Removed => "removed",
            Self::Unchanged => "unchanged",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PathRegistration {
    pub(crate) owner: String,
    pub(crate) change: Change,
    pub(crate) activation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanStateResult {
    pub(crate) app_data_removed: bool,
    pub(crate) project_state_removed: bool,
}

pub(crate) fn install_paths() -> Result<InstallPaths, AppError> {
    let source_binary = env::current_exe()
        .map_err(|err| AppError::runtime(format!("현재 rpotato 실행 경로 확인 실패: {err}")))?;
    let user_home = user_home()?;
    let user_bin = user_bin_dir(&user_home)?;
    let binary_name = if cfg!(windows) {
        "rpotato.exe"
    } else {
        "rpotato"
    };
    let project_root = layout::project_root();

    Ok(InstallPaths {
        source_binary,
        installed_binary: user_bin.join(binary_name),
        user_bin,
        user_home,
        app_data: layout::app_data_root(),
        project_state: project_root.join(".rpotato"),
        project_root,
    })
}

pub(crate) fn current_invocation_is_installed(paths: &InstallPaths) -> bool {
    path_safety::equivalent_path(&paths.source_binary, &paths.installed_binary)
}

fn user_home() -> Result<PathBuf, AppError> {
    #[cfg(windows)]
    {
        return env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or_else(|| {
                AppError::blocked("사용자 home 경로를 찾지 못해 CLI를 설치할 수 없습니다.")
            });
    }
    #[cfg(not(windows))]
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| AppError::blocked("사용자 home 경로를 찾지 못해 CLI를 설치할 수 없습니다."))
}

fn user_bin_dir(home: &Path) -> Result<PathBuf, AppError> {
    #[cfg(windows)]
    {
        return env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Programs").join("rpotato").join("bin"))
            .ok_or_else(|| {
                AppError::blocked("LOCALAPPDATA를 찾지 못해 Windows CLI를 설치할 수 없습니다.")
            });
    }
    #[cfg(not(windows))]
    {
        Ok(home.join(".local").join("bin"))
    }
}

#[cfg(test)]
#[path = "system_install/tests.rs"]
mod tests;
