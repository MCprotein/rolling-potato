use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::adapters::process::resource as process_resource;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;
use crate::runtime_core::inference::resource;

use super::now_ms;

static BACKEND_RESOURCE_SAMPLE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq)]
pub(super) struct BackendResourceSampleReport {
    pub(super) metric: observability::ResourceSampleMetric,
    pub(super) ledger_event: String,
    pub(super) pressure: resource::ResourcePressure,
}

pub(super) fn display_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn display_optional_u64_unknown(value: Option<u64>) -> String {
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

pub(super) fn record_backend_resource_sample(
    record: &BackendSidecarRecord,
    reason: &str,
) -> Result<BackendResourceSampleReport, AppError> {
    let snapshot = process_resource::sample_process(record.pid, &backend_resource_paths(record));
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
