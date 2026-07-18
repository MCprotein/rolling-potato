//! CLI installation composition and user-facing reports.

use crate::adapters::filesystem::{backend_state, runtime_mutation};
use crate::adapters::process::backend as backend_process;
use crate::adapters::system_install::{self, InstallPaths};
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::InstallCommand;

pub(crate) fn install_report(command: InstallCommand) -> Result<String, AppError> {
    let paths = system_install::install_paths()?;
    match command {
        InstallCommand::Standard => execute_install(&paths),
        InstallCommand::CleanDryRun => clean_dry_run_report(&paths),
        InstallCommand::CleanConfirmed => {
            system_install::validate_clean_targets(&paths)?;
            let _runtime_transition = runtime_mutation::acquire("clean install")?;
            require_inactive_runtime("clean install")?;
            let binary_change = system_install::install_binary(&paths)?;
            let registration = system_install::ensure_user_path(&paths)?;
            let clean_result = system_install::remove_clean_state(&paths)?;
            Ok(install_result_report(
                &paths,
                binary_change,
                registration,
                Some(clean_result),
            ))
        }
    }
}

pub(crate) fn init_environment_report() -> Result<String, AppError> {
    let paths = match system_install::install_paths() {
        Ok(paths) => paths,
        Err(err) => {
            return Ok(unavailable_environment_report(&err.message));
        }
    };
    if !system_install::current_invocation_is_installed(&paths) {
        return Ok(format!(
            "rpotato init CLI 환경\n- status: skipped\n- 이유: 현재 실행 파일이 사용자 설치 경로의 binary가 아닙니다.\n- 설치 경로: {}\n- 다음 단계: `rpotato install`을 실행하면 binary와 PATH를 자동 등록합니다.",
            paths.installed_binary.display()
        ));
    }

    let registration = match system_install::ensure_user_path(&paths) {
        Ok(registration) => registration,
        Err(err) => return Ok(unavailable_environment_report(&err.message)),
    };
    Ok(format!(
        "rpotato init CLI 환경\n- status: ready\n- binary: {}\n- PATH owner: {}\n- PATH registration: {}\n- 적용: 새 terminal부터 자동 적용\n- 현재 terminal 활성화: {}\n- RPOTATO_* override: 선택 사항이므로 전역 값을 강제하지 않음",
        paths.installed_binary.display(),
        registration.owner,
        registration.change.label(),
        registration.activation
    ))
}

fn unavailable_environment_report(reason: &str) -> String {
    format!(
        "rpotato init CLI 환경\n- status: unavailable\n- 이유: {reason}\n- runtime state 초기화는 계속 수행함"
    )
}

fn execute_install(paths: &InstallPaths) -> Result<String, AppError> {
    let binary_change = system_install::install_binary(paths)?;
    let registration = system_install::ensure_user_path(paths)?;
    Ok(install_result_report(
        paths,
        binary_change,
        registration,
        None,
    ))
}

fn install_result_report(
    paths: &InstallPaths,
    binary_change: system_install::Change,
    registration: system_install::PathRegistration,
    clean_result: Option<system_install::CleanStateResult>,
) -> String {
    let mode = if clean_result.is_some() {
        "clean"
    } else {
        "standard"
    };
    let mut lines = vec![
        format!("rpotato install ({mode})"),
        format!("- source binary: {}", paths.source_binary.display()),
        format!(
            "- installed binary: {} ({})",
            paths.installed_binary.display(),
            binary_change.label()
        ),
        format!("- PATH owner: {}", registration.owner),
        format!("- PATH registration: {}", registration.change.label()),
    ];
    if let Some(clean_result) = clean_result {
        lines.extend([
            format!(
                "- app data: {} ({})",
                paths.app_data.display(),
                removed_label(clean_result.app_data_removed)
            ),
            format!(
                "- project state: {} ({})",
                paths.project_state.display(),
                removed_label(clean_result.project_state_removed)
            ),
        ]);
    } else {
        lines.push("- 기존 config, model, backend, project state: preserved".to_string());
    }
    lines.extend([
        "- 적용: 새 terminal부터 `rpotato` 명령을 바로 사용할 수 있음".to_string(),
        format!("- 현재 terminal 활성화: {}", registration.activation),
        format!("- 다음 단계: {} init", paths.installed_binary.display()),
    ]);
    lines.join("\n")
}

fn clean_dry_run_report(paths: &InstallPaths) -> Result<String, AppError> {
    system_install::validate_clean_targets(paths)?;
    let binary_change = system_install::binary_install_plan(paths)?;
    let path_registration = system_install::user_path_change_plan(paths)?;
    Ok(format!(
        "rpotato install (clean dry-run)\n- source binary: {}\n- installed binary: {} ({})\n- PATH owner: {}\n- PATH registration: {}\n- remove app data: {} ({})\n- remove project state: {} ({})\n- runtime guard: 실제 실행 시 active backend/generation이 없어야 함\n- 현재 terminal 활성화: {}\n- 실행: rpotato install --clean --yes",
        paths.source_binary.display(),
        paths.installed_binary.display(),
        binary_change.label(),
        path_registration.owner,
        path_registration.change.label(),
        paths.app_data.display(),
        presence_label(&paths.app_data),
        paths.project_state.display(),
        presence_label(&paths.project_state),
        path_registration.activation
    ))
}

pub(crate) fn require_inactive_runtime(operation: &str) -> Result<(), AppError> {
    require_inactive_runtime_with(operation, backend_process::running_status)
}

fn require_inactive_runtime_with(
    operation: &str,
    mut running_status: impl FnMut(u32) -> Result<bool, AppError>,
) -> Result<(), AppError> {
    if let Some(record) = backend_state::read_sidecar_record()? {
        if running_status(record.pid)? {
            return Err(AppError::blocked(format!(
                "{operation} 차단\n- 이유: backend sidecar가 실행 중입니다.\n- pid: {}\n- 다음 단계: rpotato backend stop",
                record.pid,
            )));
        }
    }
    for generation in [
        backend_state::read_generation_record()?,
        backend_state::read_generation_lock_record()?,
    ]
    .into_iter()
    .flatten()
    {
        if running_status(generation.client_pid)? {
            return Err(AppError::blocked(format!(
                "{operation} 차단\n- 이유: active generation이 있습니다.\n- client pid: {}\n- 다음 단계: rpotato backend cancel",
                generation.client_pid,
            )));
        }
    }
    Ok(())
}

fn presence_label(path: &std::path::Path) -> &'static str {
    if std::fs::symlink_metadata(path).is_ok() {
        "present"
    } else {
        "missing"
    }
}

fn removed_label(removed: bool) -> &'static str {
    if removed {
        "removed"
    } else {
        "already missing"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;
    use std::path::PathBuf;

    #[test]
    fn dry_run_names_every_mutation_without_executing_it() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rpotato-install-report-{}-{nonce}",
            std::process::id()
        ));
        let paths = InstallPaths {
            source_binary: root.join("download/rpotato"),
            installed_binary: root.join("home/.local/bin/rpotato"),
            user_bin: root.join("home/.local/bin"),
            user_home: root.join("home"),
            app_data: root.join("data/rpotato"),
            project_root: root.join("project"),
            project_state: root.join("project/.rpotato"),
        };
        std::fs::create_dir_all(paths.source_binary.parent().unwrap()).unwrap();
        std::fs::write(&paths.source_binary, "binary").unwrap();

        let report = clean_dry_run_report(&paths).unwrap();

        assert!(report.contains("clean dry-run"));
        assert!(report.contains("installed binary:"));
        assert!(report.contains("(created)"));
        assert!(report.contains("PATH registration: created"));
        assert!(report.contains("remove app data:"));
        assert!(report.contains("remove project state:"));
        assert!(report.contains("--clean --yes"));
        assert!(!paths.installed_binary.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn clean_install_is_blocked_while_backend_process_is_active() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        backend_state::write_sidecar_record(&BackendSidecarRecord {
            backend_id: "llama.cpp".to_string(),
            pid: std::process::id(),
            binary_path: PathBuf::from("llama-server"),
            model_path: PathBuf::from("model.gguf"),
            model_sha256: "a".repeat(64),
            model_size_bytes: 1,
            backend_release: "test".to_string(),
            binary_sha256: "b".repeat(64),
            mmproj: "not-required-text-only".to_string(),
            host: "127.0.0.1".to_string(),
            port: 1,
            ctx_size: Some(128),
            stdout_log: PathBuf::from("stdout.log"),
            stderr_log: PathBuf::from("stderr.log"),
            started_at_ms: 1,
        })
        .unwrap();

        let err = require_inactive_runtime("clean install").unwrap_err();

        assert_eq!(err.code, 3);
        assert!(err.message.contains("backend sidecar"));
        assert!(err.message.contains("backend stop"));
    }

    #[test]
    fn clean_install_fails_closed_when_process_liveness_is_unavailable() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        backend_state::write_sidecar_record(&BackendSidecarRecord {
            backend_id: "llama.cpp".to_string(),
            pid: 42,
            binary_path: PathBuf::from("llama-server"),
            model_path: PathBuf::from("model.gguf"),
            model_sha256: "a".repeat(64),
            model_size_bytes: 1,
            backend_release: "test".to_string(),
            binary_sha256: "b".repeat(64),
            mmproj: "not-required-text-only".to_string(),
            host: "127.0.0.1".to_string(),
            port: 1,
            ctx_size: Some(128),
            stdout_log: PathBuf::from("stdout.log"),
            stderr_log: PathBuf::from("stderr.log"),
            started_at_ms: 1,
        })
        .unwrap();

        let err = require_inactive_runtime_with("clean install", |_| {
            Err(AppError::runtime("process liveness probe unavailable"))
        })
        .unwrap_err();

        assert_eq!(err.code, 1);
        assert!(err.message.contains("liveness probe unavailable"));
        assert!(backend_state::sidecar_record_path().is_file());
    }

    #[test]
    fn missing_user_home_does_not_block_runtime_init_contract() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let saved_home = std::env::var_os("HOME");
        let saved_user_profile = std::env::var_os("USERPROFILE");
        std::env::remove_var("HOME");
        std::env::remove_var("USERPROFILE");

        let report = init_environment_report().unwrap();

        restore_env("HOME", saved_home);
        restore_env("USERPROFILE", saved_user_profile);
        assert!(report.contains("status: unavailable"));
        assert!(report.contains("runtime state 초기화는 계속 수행함"));
    }

    fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            std::env::set_var(name, value);
        } else {
            std::env::remove_var(name);
        }
    }
}
