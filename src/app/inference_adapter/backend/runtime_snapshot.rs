//! Read-only backend runtime status used by interactive surfaces.

use std::time::Duration;

use crate::adapters::filesystem::backend_state;
use crate::adapters::llama_cpp::backend as llama_backend;
use crate::adapters::process::backend as backend_process;
use crate::foundation::error::AppError;

use super::{model_id_from_path, HEALTH_TIMEOUT_MS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendRuntimeSnapshot {
    pub(crate) status: &'static str,
    pub(crate) model_id: Option<String>,
    pub(crate) model_path: Option<std::path::PathBuf>,
    pub(crate) context_limit_tokens: Option<u32>,
    pub(crate) vision_ready: bool,
}

pub(crate) fn runtime_snapshot() -> Result<BackendRuntimeSnapshot, AppError> {
    let Some(record) = backend_state::read_sidecar_record()? else {
        return Ok(BackendRuntimeSnapshot {
            status: "stopped",
            model_id: None,
            model_path: None,
            context_limit_tokens: None,
            vision_ready: false,
        });
    };
    let running = backend_process::is_running(record.pid);
    let healthy = running
        && llama_backend::probe_health(
            &record.host,
            record.port,
            Duration::from_millis(HEALTH_TIMEOUT_MS),
        )
        .status
            == "healthy";
    Ok(BackendRuntimeSnapshot {
        status: if healthy { "ready" } else { "stale" },
        model_id: Some(model_id_from_path(&record.model_path)),
        model_path: Some(record.model_path),
        context_limit_tokens: record.ctx_size,
        vision_ready: record.mmproj_path.is_some(),
    })
}
