use crate::adapters::filesystem::uninstall;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
