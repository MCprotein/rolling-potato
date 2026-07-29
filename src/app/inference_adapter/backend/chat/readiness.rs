use std::time::Duration;

use crate::adapters::filesystem::backend_state;
use crate::adapters::llama_cpp::backend as llama_backend;
use crate::adapters::process::backend as backend_process;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;

use super::super::HEALTH_TIMEOUT_MS;

pub(super) fn ready_sidecar_record() -> Result<BackendSidecarRecord, AppError> {
    let Some(record) = backend_state::read_sidecar_record()? else {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: 실행 중인 sidecar record가 없습니다.\n- 다음 단계: rpotato backend start --model <path> --ctx-size 4096\n- sidecar record: {}",
            backend_state::sidecar_record_path().display()
        )));
    };
    if !backend_process::is_running(record.pid) {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: sidecar record는 있지만 process가 실행 중이 아닙니다.\n- pid: {}\n- 다음 단계: rpotato backend stop으로 stale record를 정리한 뒤 다시 시작하세요.",
            record.pid
        )));
    }

    let health = llama_backend::probe_health(
        &record.host,
        record.port,
        Duration::from_millis(HEALTH_TIMEOUT_MS),
    );
    if health.status != "healthy" {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: sidecar health check 실패\n- pid: {}\n- health: {}\n- health error: {}\n- 다음 단계: rpotato backend status로 log path를 확인하세요.",
            record.pid,
            health.status,
            health.error.unwrap_or_else(|| "없음".to_string())
        )));
    }
    Ok(record)
}
