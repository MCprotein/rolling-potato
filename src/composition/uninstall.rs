use crate::adapters::filesystem::{runtime_mutation, uninstall};
use crate::adapters::system_install::{self, InstallPaths};
use crate::composition::install;
use crate::foundation::error::AppError;
use crate::surfaces::cli::command::UninstallCommand;

pub(crate) fn uninstall_report(command: UninstallCommand) -> Result<String, AppError> {
    match command {
        UninstallCommand::Plan {
            purge_cache,
            dry_run,
        } => Ok(plan_report(purge_cache, dry_run)),
        UninstallCommand::CleanDryRun => {
            let paths = system_install::install_paths()?;
            clean_dry_run_report(&paths)
        }
        UninstallCommand::CleanConfirmed => {
            let paths = system_install::install_paths()?;
            execute_clean_uninstall(&paths)
        }
    }
}

pub(crate) fn plan_report(purge_cache: bool, dry_run: bool) -> String {
    let paths = uninstall::managed_paths();
    let mode = if purge_cache {
        "--purge-cache"
    } else {
        "--keep-cache"
    };
    let execution = if dry_run {
        "dry-run 명시됨"
    } else {
        "안전상 dry-run summary만 출력"
    };
    let mut lines = vec![
        format!("uninstall 계획 ({mode})"),
        format!("- 실행 상태: {execution}"),
        format!("- program/runtime assets: {}", paths.backends.display()),
        format!("- config: {}", paths.config.display()),
        format!("- operation log: {}", paths.operation_log.display()),
    ];

    if purge_cache {
        lines.extend([
            format!("- models: {}", paths.models.display()),
            format!("- downloads: {}", paths.downloads.display()),
            format!("- manifests: {}", paths.manifests.display()),
            format!("- state: {}", paths.state.display()),
            format!("- plugins: {}", paths.plugins.display()),
            format!("- cache: {}", paths.cache.display()),
        ]);
    } else {
        lines.extend([
            format!("- 보존: {}", paths.models.display()),
            format!("- 보존: {}", paths.downloads.display()),
            format!("- 보존: {}", paths.manifests.display()),
            format!("- 보존: {}", paths.state.display()),
            format!("- 보존: {}", paths.plugins.display()),
            format!("- 보존: {}", paths.cache.display()),
        ]);
    }

    lines.push(format!(
        "- project state는 global uninstall에서 삭제하지 않음: {}",
        paths.project_state.display()
    ));
    lines.push("삭제 실행은 아직 구현하지 않았습니다.".to_string());
    lines.join("\n")
}

fn clean_dry_run_report(paths: &InstallPaths) -> Result<String, AppError> {
    system_install::validate_clean_uninstall_targets(paths)?;
    let binary = system_install::binary_removal_plan(paths)?;
    let registration = system_install::user_path_removal_plan(paths)?;
    let mut lines = vec![
        "rpotato uninstall (clean dry-run)".to_string(),
        format!(
            "- installed binary: {} ({})",
            paths.installed_binary.display(),
            binary.label()
        ),
        format!("- PATH owner: {}", registration.owner),
        format!("- PATH registration: {}", registration.change.label()),
        format!(
            "- remove app data: {} ({})",
            paths.app_data.display(),
            presence_label(&paths.app_data)?
        ),
        format!(
            "- remove project state: {} ({})",
            paths.project_state.display(),
            presence_label(&paths.project_state)?
        ),
    ];
    if !system_install::current_invocation_is_installed(paths) {
        lines.push(format!(
            "- preserve invocation source: {} (user-owned extracted file)",
            paths.source_binary.display()
        ));
    }
    lines.extend([
        "- runtime guard: 실제 실행 시 active backend/generation이 없어야 함".to_string(),
        "- 안전 경계: rpotato 소유 PATH block과 관리형 설치·상태만 삭제".to_string(),
        "- 실행: rpotato uninstall --clean --yes".to_string(),
    ]);
    Ok(lines.join("\n"))
}

fn execute_clean_uninstall(paths: &InstallPaths) -> Result<String, AppError> {
    system_install::validate_clean_uninstall_targets(paths)?;
    let _runtime_transition = runtime_mutation::acquire("clean uninstall")?;
    install::require_inactive_runtime("clean uninstall")?;
    execute_clean_uninstall_after_guard(paths)
}

fn execute_clean_uninstall_after_guard(paths: &InstallPaths) -> Result<String, AppError> {
    let registration = system_install::remove_user_path(paths)?;
    let clean_state = system_install::remove_clean_state(paths)?;
    let binary = system_install::remove_installed_binary(paths)?;
    let mut lines = vec![
        "rpotato uninstall (clean)".to_string(),
        format!(
            "- installed binary: {} ({})",
            paths.installed_binary.display(),
            if binary.deferred_until_exit {
                "scheduled after process exit"
            } else {
                binary.change.label()
            }
        ),
        format!("- PATH owner: {}", registration.owner),
        format!("- PATH registration: {}", registration.change.label()),
        format!(
            "- app data: {} ({})",
            paths.app_data.display(),
            removed_label(clean_state.app_data_removed)
        ),
        format!(
            "- project state: {} ({})",
            paths.project_state.display(),
            removed_label(clean_state.project_state_removed)
        ),
    ];
    if !system_install::current_invocation_is_installed(paths) {
        lines.push(format!(
            "- invocation source: {} (preserved; user-owned extracted file)",
            paths.source_binary.display()
        ));
    }
    lines.push(
        if binary.deferred_until_exit {
            "- status: binary cleanup scheduled; process 종료 후 완료"
        } else {
            "- status: complete"
        }
        .to_string(),
    );
    Ok(lines.join("\n"))
}

fn presence_label(path: &std::path::Path) -> Result<&'static str, AppError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok("present"),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok("missing"),
        Err(err) => Err(AppError::runtime(format!(
            "clean uninstall target 상태 확인 실패: {} ({err})",
            path.display()
        ))),
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
    #[cfg(unix)]
    use std::path::PathBuf;

    #[test]
    fn keep_cache_plan_preserves_managed_data_and_never_executes() {
        let report = plan_report(false, false);

        assert!(report.contains("uninstall 계획 (--keep-cache)"));
        assert!(report.contains("- 보존:"));
        assert!(report.contains("삭제 실행은 아직 구현하지 않았습니다."));
    }

    #[test]
    fn purge_cache_dry_run_lists_managed_data_without_deleting() {
        let report = plan_report(true, true);

        assert!(report.contains("uninstall 계획 (--purge-cache)"));
        assert!(report.contains("- 실행 상태: dry-run 명시됨"));
        assert!(report.contains("- models:"));
        assert!(report.contains("project state는 global uninstall에서 삭제하지 않음"));
    }

    #[cfg(unix)]
    #[test]
    fn clean_dry_run_is_exact_and_read_only() {
        let paths = test_paths("dry-run");
        std::fs::create_dir_all(paths.source_binary.parent().unwrap()).unwrap();
        std::fs::write(&paths.source_binary, "source").unwrap();

        let report = clean_dry_run_report(&paths).unwrap();

        assert!(report.contains("clean dry-run"));
        assert!(report.contains("installed binary:"));
        assert!(report.contains("PATH registration: unchanged"));
        assert!(report.contains("remove app data:"));
        assert!(report.contains("remove project state:"));
        assert!(report.contains("preserve invocation source:"));
        assert!(report.contains("--clean --yes"));
        assert!(paths.source_binary.is_file());
        assert!(!paths.installed_binary.exists());
        let _ = std::fs::remove_dir_all(test_root("dry-run"));
    }

    #[cfg(unix)]
    #[test]
    fn confirmed_clean_uninstall_removes_managed_installation_and_state() {
        let paths = test_paths("confirmed");
        std::fs::create_dir_all(paths.source_binary.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&paths.app_data).unwrap();
        std::fs::create_dir_all(&paths.project_state).unwrap();
        std::fs::write(&paths.source_binary, "source").unwrap();
        system_install::install_binary(&paths).unwrap();
        system_install::ensure_user_path(&paths).unwrap();

        let report = execute_clean_uninstall_after_guard(&paths).unwrap();

        assert!(report.contains("rpotato uninstall (clean)"));
        assert!(report.contains("PATH registration: removed"));
        assert!(report.contains("status: complete"));
        assert!(!paths.installed_binary.exists());
        assert!(!paths.app_data.exists());
        assert!(!paths.project_state.exists());
        assert!(paths.source_binary.is_file());
        let _ = std::fs::remove_dir_all(test_root("confirmed"));
    }

    #[cfg(unix)]
    fn test_paths(label: &str) -> InstallPaths {
        let root = test_root(label);
        InstallPaths {
            source_binary: root.join("download/rpotato"),
            installed_binary: root.join("home/.local/bin/rpotato"),
            user_bin: root.join("home/.local/bin"),
            user_home: root.join("home"),
            app_data: root.join("data/rpotato"),
            project_root: root.join("project"),
            project_state: root.join("project/.rpotato"),
        }
    }

    #[cfg(unix)]
    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rpotato-clean-uninstall-{label}-{}",
            std::process::id()
        ))
    }
}
