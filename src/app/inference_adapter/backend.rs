use std::env;
#[cfg(test)]
use std::fs::File;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::time::{Duration, Instant};

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
mod generation_gateway;
mod generation_state;
mod installation;
mod resource_sampling;
mod runtime_snapshot;
mod sidecar;
mod support;
pub use chat::{
    cancel_generation_report, chat_once, chat_once_bounded, chat_once_bounded_with_cancel,
    chat_report, chat_stream_report, preflight_chat_ready,
};
pub(crate) use chat::{
    chat_once_for_intent, chat_once_with_input_for_intent,
    chat_once_with_input_for_intent_and_cancel, chat_once_with_input_for_intent_and_cancel_bounded,
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
use support::{
    display_optional_u128, display_optional_u32, display_vec, model_identity, now_ms,
    runtime_vision_projector_ready, vision_readiness, HEALTH_TIMEOUT_MS,
    TERMINAL_RECORD_RETENTION_MS,
};
#[cfg(test)]
use support::{runtime_binding_matches, supported_vision_readiness};

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

#[cfg(test)]
#[path = "backend/tests.rs"]
mod tests;
