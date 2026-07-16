use std::path::PathBuf;

use super::layout;

pub(crate) struct ConfigSources {
    pub(crate) directory: PathBuf,
    pub(crate) file: PathBuf,
}

pub(crate) fn discover() -> ConfigSources {
    ConfigSources {
        directory: layout::config_dir(),
        file: layout::config_file(),
    }
}
