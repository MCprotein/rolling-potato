//! llama.cpp backend facade and shared adapter contracts.

use std::path::PathBuf;

mod discovery;
mod health;
mod input_tokens;
mod request;
mod sidecar;
mod version;

pub(crate) use discovery::discover;
#[cfg(test)]
pub(crate) use discovery::is_executable;
#[cfg(test)]
use health::first_http_status_line;
pub(crate) use health::probe_health;
pub(crate) use input_tokens::{chat_input_tokens_request_body, parse_chat_input_tokens_response};
#[cfg(test)]
pub(crate) use request::chat_request_body;
pub(crate) use request::chat_request_body_for_input;
#[cfg(test)]
use request::{encode_base64, JSON_SCHEMA_REPETITION_KEYS};
pub(crate) use sidecar::sidecar_command;
pub(crate) use version::probe_version;

pub(crate) const LLAMA_CPP_BACKEND_ID: &str = "llama.cpp";
pub(crate) const DEFAULT_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_PORT: u16 = 17842;
pub(crate) const ENV_BACKEND_PATH: &str = "RPOTATO_BACKEND_LLAMA_CPP_PATH";
pub(crate) const ENV_BACKEND_PORT: &str = "RPOTATO_BACKEND_PORT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LlamaCppAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendDiscovery {
    pub(crate) adapter_id: &'static str,
    pub(crate) binary_name: &'static str,
    pub(crate) managed_path: PathBuf,
    pub(crate) selected_path: PathBuf,
    pub(crate) selected_source: &'static str,
    pub(crate) override_path: Option<PathBuf>,
    pub(crate) binary_exists: bool,
    pub(crate) binary_is_file: bool,
    pub(crate) binary_executable: bool,
    pub(crate) host: &'static str,
    pub(crate) port: u16,
    pub(crate) port_source: &'static str,
    pub(crate) health_url: String,
}

pub(crate) struct HealthProbe {
    pub(crate) status: &'static str,
    pub(crate) tcp_connected: bool,
    pub(crate) http_status_line: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendVersionProbe {
    pub(crate) status: &'static str,
    pub(crate) command: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) output: Option<String>,
    pub(crate) error: Option<String>,
}

#[cfg(test)]
#[path = "backend/tests.rs"]
mod tests;
