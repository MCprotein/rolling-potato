//! Exact reconciliation and reporting for an already-running sidecar.

use std::path::Path;

use crate::adapters::filesystem::backend_state;
use crate::adapters::process::backend as backend_process;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;
use crate::runtime_core::inference::backend::reconciliation::{
    runtime_drift, BackendRuntimeObservation, BackendRuntimeSpec,
};

use super::super::{
    display_optional_f64, display_optional_u32, display_optional_u64_unknown,
    record_backend_resource_sample,
};

pub(super) fn report(model_path: &Path, ctx_size: Option<u32>) -> Result<Option<String>, AppError> {
    let Some(record) = backend_state::read_sidecar_record()? else {
        return Ok(None);
    };
    if !backend_process::is_running(record.pid) {
        backend_state::remove_sidecar_record()?;
        return Ok(None);
    }
    ensure_matches(&record, model_path, ctx_size)?;
    let resource_sample = record_backend_resource_sample(&record, "start-existing")?;
    Ok(Some(format!(
        "backend start\n- status: already-running\n- pid: {}\n- binary: {}\n- model: {}\n- vision: {}\n- mmproj: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- stdout log: {}\n- stderr log: {}",
        record.pid,
        record.binary_path.display(),
        record.model_path.display(),
        if record.mmproj_path.is_some() {
            "ready"
        } else {
            "unavailable (text-ready)"
        },
        record
            .mmproj_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "없음".to_string()),
        record.host,
        record.port,
        display_optional_u32(record.ctx_size),
        resource_sample.metric.pressure_status,
        display_optional_f64(resource_sample.metric.process_cpu_percent),
        display_optional_u64_unknown(resource_sample.metric.average_rss_bytes),
        display_optional_u64_unknown(resource_sample.metric.peak_rss_bytes),
        display_optional_u64_unknown(resource_sample.metric.disk_bytes),
        resource_sample.ledger_event,
        record.stdout_log.display(),
        record.stderr_log.display()
    )))
}

fn ensure_matches(
    record: &BackendSidecarRecord,
    model_path: &Path,
    ctx_size: Option<u32>,
) -> Result<(), AppError> {
    let requested_model_sha256 = checksum::sha256_file(model_path)?;
    let requested_projector = crate::app::inference_adapter::model::verified_vision_projector(
        model_path,
        &requested_model_sha256,
    );
    let desired = BackendRuntimeSpec {
        model_path: model_path.to_path_buf(),
        context_limit_tokens: ctx_size.ok_or_else(|| {
            AppError::blocked("실행 중인 backend와 비교하려면 요청 context length가 필요합니다.")
        })?,
        vision_projector_path: requested_projector.map(|projector| projector.path),
    };
    let drift = runtime_drift(
        &desired,
        &BackendRuntimeObservation {
            ready: true,
            model_path: Some(record.model_path.clone()),
            context_limit_tokens: record.ctx_size,
            vision_projector_path: record.mmproj_path.clone(),
        },
    );
    if drift.is_empty() {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "backend start 차단\n- 이유: 실행 중인 backend가 요청 spec과 다릅니다.\n- drift: {drift:?}\n- 동작: 기존 backend를 먼저 중지한 뒤 정확한 model·context·mmproj로 다시 시작하세요."
        )))
    }
}
