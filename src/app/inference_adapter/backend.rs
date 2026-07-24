use std::env;
#[cfg(test)]
use std::fs::File;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use crate::adapters::filesystem::backend_state;
use crate::adapters::filesystem::layout as paths;
use crate::adapters::llama_cpp::backend as llama_backend;
use crate::adapters::llama_cpp::install as llama_install;
#[cfg(test)]
use crate::adapters::process::backend as backend_process;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
#[cfg(test)]
use crate::runtime_core::inference::backend::lifecycle::BackendGenerationRecord;
#[cfg(test)]
use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;
#[cfg(test)]
use crate::runtime_core::inference::backend::lifecycle::{
    parse_generation_record, render_generation_record,
};
use crate::runtime_core::inference::backend::BackendAdapter;
use llama_backend::LlamaCppAdapter;
#[cfg(test)]
use llama_backend::{
    DEFAULT_HOST, DEFAULT_PORT, ENV_BACKEND_PATH, ENV_BACKEND_PORT, LLAMA_CPP_BACKEND_ID,
};
use llama_install::{
    install_blockers as backend_install_blockers,
    selected_release_artifact as selected_backend_release_artifact, ArchiveDownloadStatus,
    BackendReleaseArtifact, LLAMA_CPP_RELEASE,
};
#[cfg(test)]
use llama_install::{release_artifact_for, BackendArchiveKind};

mod chat;
mod generation_state;
mod installation;
mod resource_sampling;
mod runtime_snapshot;
mod sidecar;
pub(crate) use chat::chat_once_with_input;
pub use chat::{
    cancel_generation_report, chat_once, chat_once_bounded, chat_once_bounded_with_cancel,
    chat_report, chat_stream_report, preflight_chat_ready,
};
#[cfg(test)]
use generation_state::{
    begin_active_generation, generation_cancel_requested, write_generation_terminal_record,
};
#[cfg(test)]
use generation_state::{release_generation_admission, write_backend_generation_record};
#[cfg(test)]
use installation::install_backend_from_archive;
pub use installation::{install_plan_report, install_report, verify_archive_report};
pub(crate) use runtime_snapshot::{runtime_snapshot, BackendRuntimeSnapshot};
#[cfg(test)]
use sidecar::{
    cancel_active_generation_before_stop, start_sidecar_with_timeout, terminate_with_fallback,
};
pub use sidecar::{
    doctor_report, doctor_summary, health_check_report, start_report, status_report, stop_report,
};

pub(crate) fn ensure_installed_report() -> Result<String, AppError> {
    let discovery = llama_backend::discover();
    if discovery.binary_exists && discovery.binary_is_file && discovery.binary_executable {
        return Ok(format!(
            "backend 준비 완료\n- status: already-ready\n- backend: {}\n- binary: {}\n- source: {}",
            discovery.adapter_id,
            discovery.selected_path.display(),
            discovery.selected_source
        ));
    }
    install_report()
}

const HEALTH_TIMEOUT_MS: u64 = 500;
const TERMINAL_RECORD_RETENTION_MS: u128 = 5 * 60 * 1_000;
fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "model-default".to_string())
}

fn display_optional_u128(value: Option<u128>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn model_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("unknown-model")
        .to_string()
}

fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "backend/tests.rs"]
mod tests;
