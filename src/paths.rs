use std::env;
use std::path::PathBuf;

pub fn app_data_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("rpotato");
        }
    }

    if cfg!(target_os = "windows") {
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("rpotato");
        }
    }

    if let Some(xdg_data_home) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg_data_home).join("rpotato");
    }

    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("rpotato");
    }

    PathBuf::from(".rpotato-data")
}

pub fn managed_backend_path() -> PathBuf {
    let binary = if cfg!(target_os = "windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    };

    app_data_root()
        .join("backends")
        .join("llama.cpp")
        .join(binary)
}

pub fn models_dir() -> PathBuf {
    app_data_root().join("models")
}

pub fn downloads_dir() -> PathBuf {
    app_data_root().join("downloads")
}

pub fn manifests_dir() -> PathBuf {
    app_data_root().join("manifests")
}
