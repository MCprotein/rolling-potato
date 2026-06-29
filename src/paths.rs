use std::env;
use std::path::PathBuf;

pub fn app_data_root() -> PathBuf {
    if let Some(path) = env::var_os("RPOTATO_DATA_HOME") {
        return PathBuf::from(path);
    }

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

pub fn config_dir() -> PathBuf {
    app_data_root().join("config")
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn backends_dir() -> PathBuf {
    app_data_root().join("backends")
}

pub fn models_dir() -> PathBuf {
    app_data_root().join("models")
}

pub fn model_registry_dir() -> PathBuf {
    models_dir().join("registry")
}

pub fn downloads_dir() -> PathBuf {
    app_data_root().join("downloads")
}

pub fn manifests_dir() -> PathBuf {
    app_data_root().join("manifests")
}

pub fn logs_dir() -> PathBuf {
    app_data_root().join("logs")
}

pub fn operation_log_file() -> PathBuf {
    logs_dir().join("operation.log")
}

pub fn state_dir() -> PathBuf {
    app_data_root().join("state")
}

pub fn current_state_file() -> PathBuf {
    state_dir().join("current-state.json")
}

pub fn runtime_evidence_file() -> PathBuf {
    state_dir().join("runtime-evidence.jsonl")
}

pub fn observability_db_file() -> PathBuf {
    state_dir().join("observability.sqlite")
}

pub fn runtime_ledger_file() -> PathBuf {
    state_dir().join("runtime-ledger.jsonl")
}

pub fn plugins_dir() -> PathBuf {
    app_data_root().join("plugins")
}

pub fn imported_plugins_dir() -> PathBuf {
    plugins_dir().join("imported")
}

pub fn plugin_data_dir() -> PathBuf {
    plugins_dir().join("data")
}

pub fn cache_dir() -> PathBuf {
    app_data_root().join("cache")
}

pub fn project_state_dir() -> PathBuf {
    project_root().join(".rpotato")
}

pub fn project_root() -> PathBuf {
    env::var_os("RPOTATO_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn project_evidence_dir() -> PathBuf {
    project_state_dir().join("evidence")
}

pub fn project_session_ledger_file() -> PathBuf {
    project_state_dir().join("session-ledger.jsonl")
}
