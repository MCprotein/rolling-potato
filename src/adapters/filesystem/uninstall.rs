use std::path::PathBuf;

use super::layout;

pub(crate) struct ManagedUninstallPaths {
    pub(crate) backends: PathBuf,
    pub(crate) config: PathBuf,
    pub(crate) operation_log: PathBuf,
    pub(crate) models: PathBuf,
    pub(crate) downloads: PathBuf,
    pub(crate) manifests: PathBuf,
    pub(crate) state: PathBuf,
    pub(crate) plugins: PathBuf,
    pub(crate) cache: PathBuf,
    pub(crate) project_state: PathBuf,
}

pub(crate) fn managed_paths() -> ManagedUninstallPaths {
    ManagedUninstallPaths {
        backends: layout::backends_dir(),
        config: layout::config_dir(),
        operation_log: layout::operation_log_file(),
        models: layout::models_dir(),
        downloads: layout::downloads_dir(),
        manifests: layout::manifests_dir(),
        state: layout::state_dir(),
        plugins: layout::plugins_dir(),
        cache: layout::cache_dir(),
        project_state: layout::project_state_dir(),
    }
}
