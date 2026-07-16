use super::layout as paths;

pub fn status_summary() -> String {
    format!("app data root {}", paths::app_data_root().display())
}

pub fn status_report() -> String {
    format!(
        "cache 상태\n- app data root: {}\n- models: {}\n- downloads: {}\n- manifests: {}",
        paths::app_data_root().display(),
        paths::models_dir().display(),
        paths::downloads_dir().display(),
        paths::manifests_dir().display()
    )
}
