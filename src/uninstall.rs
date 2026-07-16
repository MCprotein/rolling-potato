use crate::adapters::filesystem::layout as paths;

pub fn plan_report(purge_cache: bool, dry_run: bool) -> String {
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
        format!(
            "- program/runtime assets: {}",
            paths::backends_dir().display()
        ),
        format!("- config: {}", paths::config_dir().display()),
        format!("- operation log: {}", paths::operation_log_file().display()),
    ];

    if purge_cache {
        lines.extend([
            format!("- models: {}", paths::models_dir().display()),
            format!("- downloads: {}", paths::downloads_dir().display()),
            format!("- manifests: {}", paths::manifests_dir().display()),
            format!("- state: {}", paths::state_dir().display()),
            format!("- plugins: {}", paths::plugins_dir().display()),
            format!("- cache: {}", paths::cache_dir().display()),
        ]);
    } else {
        lines.extend([
            format!("- 보존: {}", paths::models_dir().display()),
            format!("- 보존: {}", paths::downloads_dir().display()),
            format!("- 보존: {}", paths::manifests_dir().display()),
            format!("- 보존: {}", paths::state_dir().display()),
            format!("- 보존: {}", paths::plugins_dir().display()),
            format!("- 보존: {}", paths::cache_dir().display()),
        ]);
    }

    lines.push(format!(
        "- project state는 global uninstall에서 삭제하지 않음: {}",
        paths::project_state_dir().display()
    ));
    lines.push("삭제 실행은 아직 구현하지 않았습니다.".to_string());
    lines.join("\n")
}
