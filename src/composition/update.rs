//! Update availability and verified self-update orchestration.

use std::time::Duration;

use crate::adapters::filesystem::runtime_mutation;
use crate::adapters::{github_release, system_install};
use crate::foundation::error::AppError;
use crate::runtime_core::update::{classify_update, UpdateAvailability};

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn startup_notice() -> Option<String> {
    #[cfg(debug_assertions)]
    if std::env::var_os("RPOTATO_TEST_SKIP_UPDATE_CHECK").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        return None;
    }

    let release = github_release::cached_latest_release(Duration::from_millis(1500)).ok()?;
    match classify_update(CURRENT_VERSION, &release.tag, &release.release_url).ok()? {
        UpdateAvailability::Available(available) => Some(format!(
            "새 rpotato 버전이 있습니다: {} → {}\n/update 를 입력하면 SHA-256 검증 후 바로 업데이트합니다.\n{}",
            CURRENT_VERSION, available.tag, available.release_url
        )),
        UpdateAvailability::Current { .. } => None,
    }
}

pub(crate) fn check_report() -> Result<String, AppError> {
    let release = github_release::fetch_latest_release(Duration::from_secs(10))?;
    Ok(match classify_update(CURRENT_VERSION, &release.tag, &release.release_url)? {
        UpdateAvailability::Available(available) => format!(
            "rpotato update check\n- status: available\n- current: {CURRENT_VERSION}\n- latest: {}\n- release: {}\n- 다음 단계: `rpotato update`",
            available.tag, available.release_url
        ),
        UpdateAvailability::Current { latest, .. } => format!(
            "rpotato update check\n- status: current\n- current: {CURRENT_VERSION}\n- latest: {}.{}.{}",
            latest.major, latest.minor, latest.patch
        ),
    })
}

pub(crate) fn update_report() -> Result<String, AppError> {
    #[cfg(debug_assertions)]
    if let Some(report) = std::env::var_os("RPOTATO_TEST_UPDATE_REPORT") {
        return Ok(report.to_string_lossy().into_owned());
    }

    let release = github_release::fetch_latest_release(Duration::from_secs(10))?;
    let UpdateAvailability::Available(available) =
        classify_update(CURRENT_VERSION, &release.tag, &release.release_url)?
    else {
        return Ok(format!(
            "rpotato update\n- status: current\n- version: {CURRENT_VERSION}\n- 변경 없음"
        ));
    };
    let paths = system_install::install_paths()?;
    let result = with_update_transition(|| {
        system_install::validate_installed_update_target(&paths)?;
        let staged_binary = github_release::download_release_binary(&release)?;
        system_install::update_installed_binary(&paths, &staged_binary)
    })?;
    let (status, next_step) = match result {
        system_install::BinaryUpdateResult::Applied => (
            "updated",
            "현재 TUI라면 /quit 후 `rpotato`를 다시 실행하세요.",
        ),
        #[cfg(windows)]
        system_install::BinaryUpdateResult::DeferredUntilExit => (
            "scheduled",
            "현재 프로세스를 종료하면 교체됩니다. 종료 후 `rpotato`를 다시 실행하세요.",
        ),
    };
    Ok(format!(
        "rpotato update\n- status: {status}\n- previous: {CURRENT_VERSION}\n- installed: {}\n- binary: {}\n- integrity: SHA-256 verified\n- 다음 단계: {next_step}",
        available.tag,
        paths.installed_binary.display()
    ))
}

fn with_update_transition<T>(
    operation: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    let _runtime_transition = runtime_mutation::acquire("self update")?;
    operation()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_check_failure_never_blocks_the_tui() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-update-startup-failure-{}",
            std::process::id()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", &root);
        std::env::set_var("RPOTATO_TEST_LATEST_RELEASE_JSON", "not-json");

        let notice = startup_notice();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_TEST_LATEST_RELEASE_JSON");
        let _ = std::fs::remove_dir_all(root);
        assert!(notice.is_none());
    }

    #[test]
    fn self_update_uses_the_shared_runtime_mutation_lock() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-update-transition-lock-{}",
            std::process::id()
        ));
        std::env::set_var("RPOTATO_DATA_HOME", &root);
        let held = runtime_mutation::acquire("self update test").unwrap();
        let mut entered = false;

        let error = with_update_transition(|| {
            entered = true;
            Ok(())
        })
        .unwrap_err();

        drop(held);
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = std::fs::remove_dir_all(root);
        assert_eq!(error.code, 3);
        assert!(!entered, "mutation must not begin without the shared lease");
    }
}
