use std::env;
#[cfg(test)]
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
#[cfg(test)]
use crate::runtime_core::inference::backend::lifecycle::BackendGenerationRecord;
use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;
#[cfg(test)]
use crate::runtime_core::inference::backend::lifecycle::{
    parse_generation_record, render_generation_record,
};
use crate::runtime_core::inference::backend::BackendAdapter;
use crate::runtime_core::inference::resource;
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
mod sidecar;
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
#[cfg(test)]
use sidecar::{
    cancel_active_generation_before_stop, start_sidecar_with_timeout, terminate_with_fallback,
};
pub use sidecar::{
    doctor_report, doctor_summary, health_check_report, start_report, status_report, stop_report,
};

const HEALTH_TIMEOUT_MS: u64 = 500;
const TERMINAL_RECORD_RETENTION_MS: u128 = 5 * 60 * 1_000;
static BACKEND_RESOURCE_SAMPLE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq)]
struct BackendResourceSampleReport {
    metric: observability::ResourceSampleMetric,
    ledger_event: String,
    pressure: resource::ResourcePressure,
}

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

fn display_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn display_optional_u64_unknown(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn backend_resource_paths(record: &BackendSidecarRecord) -> Vec<PathBuf> {
    vec![
        record.binary_path.clone(),
        record.model_path.clone(),
        record.stdout_log.clone(),
        record.stderr_log.clone(),
    ]
}

fn record_backend_resource_sample(
    record: &BackendSidecarRecord,
    reason: &str,
) -> Result<BackendResourceSampleReport, AppError> {
    let snapshot = resource::sample_process(record.pid, &backend_resource_paths(record));
    let recorded_at_ms = now_ms();
    let sample_nonce = format!(
        "{}-{}",
        std::process::id(),
        BACKEND_RESOURCE_SAMPLE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let event_id = state::record_event(
        "backend.resource.sampled",
        "backend sidecar resource sample 기록",
        &format!(
            "reason={} sample_nonce={} pid={} backend={} cpu_percent={} average_rss_bytes={} peak_rss_bytes={} disk_bytes={} sample_count={} pressure_status={}",
            reason,
            sample_nonce,
            record.pid,
            record.backend_id,
            display_optional_f64(snapshot.process_cpu_percent),
            display_optional_u64_unknown(snapshot.average_rss_bytes),
            display_optional_u64_unknown(snapshot.peak_rss_bytes),
            display_optional_u64_unknown(snapshot.disk_bytes),
            snapshot.sample_count,
            snapshot.pressure.as_str()
        ),
    )?;
    let identity = ledger::validated_current_identity()?;
    let metric = observability::ResourceSampleMetric {
        resource_sample_id: format!("resource-sample-{event_id}"),
        session_id: identity.session_id,
        backend_id: record.backend_id.clone(),
        pid: snapshot.pid,
        process_cpu_percent: snapshot.process_cpu_percent,
        average_rss_bytes: snapshot.average_rss_bytes,
        peak_rss_bytes: snapshot.peak_rss_bytes,
        disk_bytes: snapshot.disk_bytes,
        sample_count: snapshot.sample_count,
        pressure_status: snapshot.pressure.as_str().to_string(),
        recorded_at_ms,
    };
    observability::record_resource_sample(&metric)?;

    Ok(BackendResourceSampleReport {
        metric,
        ledger_event: event_id,
        pressure: snapshot.pressure,
    })
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
