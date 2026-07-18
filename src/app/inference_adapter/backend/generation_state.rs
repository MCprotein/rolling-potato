use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::{backend_state, runtime_mutation};
use crate::adapters::process::backend as backend_process;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::backend::admission::{GenerationAdmission, GenerationRelease};
use crate::runtime_core::inference::backend::lifecycle::{
    render_generation_record, BackendGenerationRecord, BackendGenerationTerminalRecord,
    BackendSidecarRecord,
};

use super::{now_ms, TERMINAL_RECORD_RETENTION_MS};

static GENERATION_ADMISSION_STATE: Mutex<GenerationAdmission> =
    Mutex::new(GenerationAdmission::new());

pub(super) struct ActiveGenerationGuard {
    pub(super) generation_id: String,
    pub(super) finished: bool,
}

impl Drop for ActiveGenerationGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = release_generation_admission(&self.generation_id);
        }
    }
}

impl ActiveGenerationGuard {
    pub(super) fn finish(mut self) -> Result<(), AppError> {
        release_generation_admission(&self.generation_id)?;
        self.finished = true;
        Ok(())
    }
}

pub(super) fn begin_active_generation(
    sidecar: &BackendSidecarRecord,
    timeout_ms: u32,
    streaming_display: bool,
) -> Result<BackendGenerationRecord, AppError> {
    let mut admission = GENERATION_ADMISSION_STATE
        .lock()
        .map_err(|_| AppError::runtime("backend generation admission lock poisoned"))?;
    let runtime_transition = runtime_mutation::acquire("backend generation begin")?;
    backend_state::prune_generation_terminal_records(now_ms(), TERMINAL_RECORD_RETENTION_MS);
    let mut publish_primary = true;
    if let Some(active) = backend_state::read_generation_record()? {
        if backend_process::is_running(active.client_pid) {
            if admission.can_join(&active, std::process::id(), sidecar.pid) {
                publish_primary = false;
            } else {
                return Err(AppError::blocked(format!(
                    "backend chat 차단\n- 이유: 이미 active generation이 있습니다.\n- generation id: {}\n- client pid: {}\n- sidecar pid: {}\n- 다음 단계: rpotato backend cancel",
                    active.generation_id, active.client_pid, active.sidecar_pid
                )));
            }
        } else {
            remove_generation_state_if_owned(&active.generation_id);
            state::record_event(
                "backend.generation.stale.cleaned",
                "stale backend generation record 정리",
                &format!(
                    "generation_id={} client_pid={} sidecar_pid={} reason=next-generation",
                    active.generation_id, active.client_pid, active.sidecar_pid
                ),
            )?;
        }
    } else if let Some(lock) = backend_state::read_generation_lock_record()? {
        if backend_process::is_running(lock.client_pid) {
            return Err(AppError::blocked(format!(
                "backend chat 차단\n- 이유: generation lease가 publish 중입니다.\n- generation id: {}\n- client pid: {}\n- sidecar pid: {}\n- 다음 단계: 잠시 후 다시 시도하거나 rpotato backend cancel",
                lock.generation_id, lock.client_pid, lock.sidecar_pid
            )));
        }
        remove_generation_state_if_owned(&lock.generation_id);
        state::record_event(
            "backend.generation.stale.cleaned",
            "publish 전 중단된 backend generation lease 정리",
            &format!(
                "generation_id={} client_pid={} sidecar_pid={} reason=unpublished-lock-owner-not-running",
                lock.generation_id, lock.client_pid, lock.sidecar_pid
            ),
        )?;
    }

    let record = BackendGenerationRecord {
        generation_id: format!(
            "generation-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            std::process::id()
        ),
        client_pid: std::process::id(),
        sidecar_pid: sidecar.pid,
        started_at_ms: now_ms(),
        timeout_ms,
        streaming_display,
    };
    if publish_primary {
        backend_state::acquire_generation_lock(&record)?;
        if let Err(err) = write_backend_generation_record(&record) {
            let _ = backend_state::remove_generation_lock_if_owned(&record.generation_id);
            return Err(err);
        }
    }
    if !admission.register(record.generation_id.clone(), publish_primary) {
        if publish_primary {
            backend_state::remove_generation_state_if_owned(&record.generation_id)?;
        }
        return Err(AppError::blocked(
            "backend generation admission id collision",
        ));
    }
    drop(runtime_transition);
    Ok(record)
}

pub(super) fn write_backend_generation_record(
    record: &BackendGenerationRecord,
) -> Result<(), AppError> {
    let path = backend_state::generation_record_path();
    let contents = render_generation_record(record);
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, contents.as_bytes())
}

pub(super) fn generation_cancel_requested(generation_id: &str) -> Result<bool, AppError> {
    let Some(cancel_generation_id) = backend_state::read_cancel_generation_id()? else {
        return Ok(false);
    };
    if cancel_generation_id == generation_id {
        return Ok(true);
    }
    let admission = GENERATION_ADMISSION_STATE
        .lock()
        .map_err(|_| AppError::runtime("backend generation admission lock poisoned"))?;
    Ok(admission.cancellation_applies(&cancel_generation_id, generation_id))
}

pub(super) fn write_generation_cancel_marker(generation_id: &str) -> Result<(), AppError> {
    let marker = format!(
        "generation_id={}\nrequested_at_ms={}\nrequester_pid={}\n",
        generation_id,
        now_ms(),
        std::process::id()
    );
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &backend_state::generation_cancel_path(),
        marker.as_bytes(),
    )
}

pub(super) fn write_generation_terminal_record(
    generation_id: &str,
    outcome: &str,
    lifecycle_event: &str,
) -> Result<(), AppError> {
    let record = BackendGenerationTerminalRecord {
        generation_id: generation_id.to_string(),
        outcome: outcome.to_string(),
        lifecycle_event: lifecycle_event.to_string(),
        recorded_at_ms: now_ms(),
    };
    let contents = format!(
        "generation_id={}\noutcome={}\nlifecycle_event={}\nrecorded_at_ms={}\n",
        record.generation_id, record.outcome, record.lifecycle_event, record.recorded_at_ms
    );
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &backend_state::generation_terminal_path(generation_id),
        contents.as_bytes(),
    )
}

pub(super) fn wait_for_generation_terminal(
    generation_id: &str,
    timeout: Duration,
) -> Result<Option<BackendGenerationTerminalRecord>, AppError> {
    let started = Instant::now();
    loop {
        if let Some(record) = backend_state::read_generation_terminal_record(generation_id)? {
            return Ok(Some(record));
        }
        if started.elapsed() >= timeout {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn wait_for_generation_group_release(
    primary_generation_id: &str,
    timeout: Duration,
) -> Result<bool, AppError> {
    let started = Instant::now();
    loop {
        let released = backend_state::read_generation_record()?
            .is_none_or(|record| record.generation_id != primary_generation_id);
        if released {
            return Ok(true);
        }
        if started.elapsed() >= timeout {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub(super) fn remove_generation_state_if_owned(generation_id: &str) {
    let _ = backend_state::remove_generation_state_if_owned(generation_id);
}

pub(super) fn release_generation_admission(generation_id: &str) -> Result<(), AppError> {
    let mut admission = GENERATION_ADMISSION_STATE
        .lock()
        .map_err(|_| AppError::runtime("backend generation admission lock poisoned"))?;
    match admission.release(generation_id) {
        Ok(GenerationRelease::Untracked) => {
            backend_state::remove_generation_state_if_owned(generation_id)
        }
        Ok(GenerationRelease::Retained) => Ok(()),
        Ok(GenerationRelease::Last {
            primary_generation_id,
        }) => backend_state::remove_generation_state_if_owned(&primary_generation_id),
        Err(message) => Err(AppError::blocked(format!("{message}: {generation_id}"))),
    }
}
