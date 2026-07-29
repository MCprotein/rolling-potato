//! Backend binary discovery and platform executable checks.

use std::env;
use std::path::{Path, PathBuf};

use super::{
    BackendDiscovery, LlamaCppAdapter, DEFAULT_HOST, DEFAULT_PORT, ENV_BACKEND_PATH,
    ENV_BACKEND_PORT, LLAMA_CPP_BACKEND_ID,
};
use crate::adapters::filesystem::layout as paths;
use crate::runtime_core::inference::backend::BackendAdapter;

impl BackendAdapter for LlamaCppAdapter {
    fn id(&self) -> &'static str {
        LLAMA_CPP_BACKEND_ID
    }

    fn binary_name(&self) -> &'static str {
        if cfg!(target_os = "windows") {
            "llama-server.exe"
        } else {
            "llama-server"
        }
    }

    fn managed_binary_path(&self) -> PathBuf {
        paths::managed_backend_path()
    }

    fn default_host(&self) -> &'static str {
        DEFAULT_HOST
    }

    fn default_port(&self) -> u16 {
        DEFAULT_PORT
    }
}

pub(crate) fn discover() -> BackendDiscovery {
    let adapter = LlamaCppAdapter;
    let managed_path = adapter.managed_binary_path();
    let override_path = env::var_os(ENV_BACKEND_PATH).map(PathBuf::from);
    let (selected_path, selected_source) = match &override_path {
        Some(path) => (path.clone(), "env override"),
        None => (managed_path.clone(), "managed"),
    };
    let (port, port_source) = configured_port(adapter.default_port());
    let health_url = format!("http://{}:{}/health", adapter.default_host(), port);

    BackendDiscovery {
        adapter_id: adapter.id(),
        binary_name: adapter.binary_name(),
        managed_path,
        selected_path: selected_path.clone(),
        selected_source,
        override_path,
        binary_exists: selected_path.exists(),
        binary_is_file: selected_path.is_file(),
        binary_executable: is_executable(&selected_path),
        host: adapter.default_host(),
        port,
        port_source,
        health_url,
    }
}

fn configured_port(default_port: u16) -> (u16, &'static str) {
    let Some(raw_port) = env::var_os(ENV_BACKEND_PORT) else {
        return (default_port, "default");
    };
    let Some(raw_port) = raw_port.to_str() else {
        return (default_port, "invalid env, default");
    };
    match raw_port.parse::<u16>() {
        Ok(port) if port > 0 => (port, "env override"),
        _ => (default_port, "invalid env, default"),
    }
}

#[cfg(unix)]
pub(crate) fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub(crate) fn is_executable(path: &Path) -> bool {
    path.is_file()
}
