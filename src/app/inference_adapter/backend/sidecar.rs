use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

use crate::adapters::filesystem::{backend_state, layout as paths};
use crate::adapters::llama_cpp::backend as llama_backend;
use crate::adapters::llama_cpp::install::LLAMA_CPP_RELEASE;
use crate::adapters::process::backend as backend_process;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::backend::lifecycle::BackendSidecarRecord;
use crate::runtime_core::inference::backend::BackendAdapter;
use llama_backend::{LlamaCppAdapter, ENV_BACKEND_PATH, LLAMA_CPP_BACKEND_ID};

use super::generation_state::{wait_for_generation_terminal, write_generation_cancel_marker};
use super::resource_sampling::{
    display_optional_f64, display_optional_u64_unknown, record_backend_resource_sample,
};
use super::{display_optional_u32, model_id_from_path, now_ms, HEALTH_TIMEOUT_MS};

const STARTUP_TIMEOUT_MS: u64 = 60_000;
const STOP_TIMEOUT_MS: u64 = 5_000;
const STOP_CANCEL_WAIT_MS: u64 = 5_000;
const ENV_BACKEND_START_TRACE: &str = "RPOTATO_TEST_BACKEND_START_TRACE";

mod startup;
pub(super) use startup::start_sidecar_with_timeout;

pub(super) fn trace_backend_start(message: &str) {
    let Some(path) = env::var_os(ENV_BACKEND_START_TRACE) else {
        return;
    };
    let Ok(mut trace) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(trace, "{message}");
    let _ = trace.flush();
}

pub fn doctor_summary() -> String {
    let discovery = llama_backend::discover();
    if discovery.binary_exists && discovery.binary_is_file {
        format!(
            "llama.cpp backend 발견 ({}, source: {})",
            discovery.selected_path.display(),
            discovery.selected_source
        )
    } else {
        format!(
            "llama.cpp backend 미설치 (selected: {}, source: {})",
            discovery.selected_path.display(),
            discovery.selected_source
        )
    }
}

pub fn doctor_report() -> String {
    let discovery = llama_backend::discover();
    let version_probe = llama_backend::probe_version(&discovery);
    let executable_status = if discovery.binary_executable {
        "yes"
    } else {
        "no"
    };
    let override_status = discovery
        .override_path
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "없음".to_string());
    let install_status = if discovery.binary_exists && discovery.binary_is_file {
        "binary present"
    } else {
        "binary missing"
    };

    format!(
        "backend 진단\n- adapter: {}\n- binary name: {}\n- managed binary: {}\n- selected binary: {}\n- selected source: {}\n- override env {}: {}\n- binary exists: {}\n- binary is file: {}\n- executable bit: {}\n- host: {}\n- port: {} ({})\n- health URL: {}\n- install status: {}\n- version detection: {}\n- version command: {}\n- version exit code: {}\n- version output: {}\n- version error: {}\n- install gate: backend install-plan에서 현재 platform artifact, release URL, checksum, size를 확인합니다.",
        discovery.adapter_id,
        discovery.binary_name,
        discovery.managed_path.display(),
        discovery.selected_path.display(),
        discovery.selected_source,
        ENV_BACKEND_PATH,
        override_status,
        discovery.binary_exists,
        discovery.binary_is_file,
        executable_status,
        discovery.host,
        discovery.port,
        discovery.port_source,
        discovery.health_url,
        install_status,
        version_probe.status,
        version_probe.command,
        version_probe
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "없음".to_string()),
        version_probe.output.unwrap_or_else(|| "없음".to_string()),
        version_probe.error.unwrap_or_else(|| "없음".to_string())
    )
}

pub fn start_report(model_path: &str, ctx_size: Option<u32>) -> Result<String, AppError> {
    start_sidecar_with_timeout(
        model_path,
        ctx_size,
        Duration::from_millis(STARTUP_TIMEOUT_MS),
    )
}

pub fn status_report() -> Result<String, AppError> {
    let Some(record) = backend_state::read_sidecar_record()? else {
        return Ok(format!(
            "backend status\n- status: stopped\n- sidecar record: {}\n- 다음 단계: rpotato backend start --model <path> [--ctx-size <tokens>]",
            backend_state::sidecar_record_path().display()
        ));
    };

    let running = backend_process::is_running(record.pid);
    let health = if running {
        Some(llama_backend::probe_health(
            &record.host,
            record.port,
            Duration::from_millis(HEALTH_TIMEOUT_MS),
        ))
    } else {
        None
    };
    let health_status = health
        .as_ref()
        .map(|probe| probe.status)
        .unwrap_or("not-run");
    let health_error = health
        .and_then(|probe| probe.error)
        .unwrap_or_else(|| "없음".to_string());
    let status = if running { "running" } else { "stale" };
    let resource_sample = if running {
        Some(record_backend_resource_sample(&record, "status")?)
    } else {
        None
    };

    Ok(format!(
        "backend status\n- status: {}\n- backend: {}\n- pid: {}\n- binary: {}\n- model: {}\n- vision: {}\n- mmproj: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- health: {}\n- health error: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- stdout log: {}\n- stderr log: {}\n- sidecar record: {}",
        status,
        record.backend_id,
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
        health_status,
        health_error,
        resource_sample
            .as_ref()
            .map(|sample| sample.metric.pressure_status.as_str())
            .unwrap_or("not-sampled"),
        display_optional_f64(resource_sample.as_ref().and_then(|sample| sample.metric.process_cpu_percent)),
        display_optional_u64_unknown(
            resource_sample
                .as_ref()
                .and_then(|sample| sample.metric.average_rss_bytes)
        ),
        display_optional_u64_unknown(
            resource_sample
                .as_ref()
                .and_then(|sample| sample.metric.peak_rss_bytes)
        ),
        display_optional_u64_unknown(
            resource_sample
                .as_ref()
                .and_then(|sample| sample.metric.disk_bytes)
        ),
        resource_sample
            .as_ref()
            .map(|sample| sample.ledger_event.as_str())
            .unwrap_or("not-recorded"),
        record.stdout_log.display(),
        record.stderr_log.display(),
        backend_state::sidecar_record_path().display()
    ))
}

pub fn stop_report() -> Result<String, AppError> {
    let Some(record) = backend_state::read_sidecar_record()? else {
        return Ok(format!(
            "backend stop\n- status: stopped\n- sidecar record: {}\n- 동작: 실행 중인 managed sidecar record가 없어 no-op입니다.",
            backend_state::sidecar_record_path().display()
        ));
    };

    if !backend_process::is_running(record.pid) {
        backend_state::remove_sidecar_record()?;
        let event_id = state::record_event(
            "backend.sidecar.stop.stale",
            "stale backend sidecar record 제거",
            &format!("pid={} binary={}", record.pid, record.binary_path.display()),
        )?;
        return Ok(format!(
            "backend stop\n- status: stale-record-removed\n- pid: {}\n- sidecar record: {}\n- ledger event: {}",
            record.pid,
            backend_state::sidecar_record_path().display(),
            event_id
        ));
    }

    let command_matched = backend_process::command_matches(
        record.pid,
        &record.binary_path,
        LlamaCppAdapter.binary_name(),
        record.backend_id == LLAMA_CPP_BACKEND_ID,
    );

    let generation_outcome = cancel_active_generation_before_stop(&record)?;

    terminate_process_with_fallback(record.pid)?;
    backend_state::remove_sidecar_record()?;
    let event_id = state::record_event(
        "backend.sidecar.stop.completed",
        "backend sidecar 종료 완료",
        &format!(
            "pid={} binary={} command_matched={} generation_outcome={}",
            record.pid,
            record.binary_path.display(),
            command_matched,
            generation_outcome
        ),
    )?;

    Ok(format!(
        "backend stop\n- status: stopped\n- pid: {}\n- command matched: {}\n- generation outcome: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
        record.pid,
        command_matched,
        generation_outcome,
        record.stdout_log.display(),
        record.stderr_log.display(),
        event_id
    ))
}

fn terminate_process_with_fallback(pid: u32) -> Result<(), AppError> {
    terminate_with_fallback(
        || backend_process::terminate(pid, false),
        || backend_process::terminate(pid, true),
        || backend_process::running_status(pid),
        || backend_process::wait_until_stopped(pid, Duration::from_millis(STOP_TIMEOUT_MS)),
        pid,
    )
}

pub(super) fn terminate_with_fallback(
    graceful: impl FnOnce() -> Result<(), AppError>,
    force: impl FnOnce() -> Result<(), AppError>,
    is_running: impl Fn() -> Result<bool, AppError>,
    wait_until_stopped: impl Fn() -> Result<bool, AppError>,
    pid: u32,
) -> Result<(), AppError> {
    let graceful_succeeded = graceful().is_ok();
    let stopped = if graceful_succeeded {
        wait_until_stopped()?
    } else {
        !is_running()?
    };
    if stopped {
        return Ok(());
    }

    if let Err(error) = force() {
        if is_running()? {
            return Err(error);
        }
        return Ok(());
    }
    if wait_until_stopped()? {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "backend stop 실패\n- pid: {pid}\n- 이유: graceful/force 종료 후에도 process가 남아 있습니다."
        )))
    }
}

pub(super) fn cancel_active_generation_before_stop(
    record: &BackendSidecarRecord,
) -> Result<String, AppError> {
    let mut generation_outcome = "none".to_string();
    if let Some(generation) = backend_state::read_generation_record()? {
        if generation.sidecar_pid == record.pid
            && backend_process::is_running(generation.client_pid)
        {
            write_generation_cancel_marker(&generation.generation_id)?;
            state::record_event(
                "backend.generation.cancel.requested",
                "backend stop 전 generation 취소 요청",
                &format!(
                    "generation_id={} client_pid={} sidecar_pid={} requester=backend-stop",
                    generation.generation_id, generation.client_pid, generation.sidecar_pid
                ),
            )?;
            if let Some(terminal) = wait_for_generation_terminal(
                &generation.generation_id,
                Duration::from_millis(STOP_CANCEL_WAIT_MS),
            )? {
                generation_outcome = terminal.outcome;
                backend_state::remove_generation_state_if_owned(&generation.generation_id)?;
                backend_state::remove_generation_terminal_record(&generation.generation_id)?;
            } else {
                generation_outcome = "forced-sidecar-stop".to_string();
                state::record_event(
                    "backend.generation.cancel.force-stop",
                    "generation cancellation acknowledgement timeout",
                    &format!(
                        "generation_id={} client_pid={} sidecar_pid={} wait_ms={}",
                        generation.generation_id,
                        generation.client_pid,
                        generation.sidecar_pid,
                        STOP_CANCEL_WAIT_MS
                    ),
                )?;
            }
        } else if generation.sidecar_pid == record.pid {
            backend_state::remove_generation_state_if_owned(&generation.generation_id)?;
            generation_outcome = "stale-generation-cleaned".to_string();
            state::record_event(
                "backend.generation.stale.cleaned",
                "backend stop 전 stale generation 정리",
                &format!(
                    "generation_id={} client_pid={} sidecar_pid={} reason=backend-stop-client-not-running",
                    generation.generation_id, generation.client_pid, generation.sidecar_pid
                ),
            )?;
        }
    }
    Ok(generation_outcome)
}

pub fn health_check_report() -> String {
    let discovery = llama_backend::discover();
    let probe = llama_backend::probe_health(
        discovery.host,
        discovery.port,
        Duration::from_millis(HEALTH_TIMEOUT_MS),
    );

    format!(
        "backend health check\n- adapter: {}\n- selected binary: {}\n- selected source: {}\n- health URL: {}\n- timeout ms: {}\n- status: {}\n- tcp connected: {}\n- http status line: {}\n- error: {}",
        discovery.adapter_id,
        discovery.selected_path.display(),
        discovery.selected_source,
        discovery.health_url,
        HEALTH_TIMEOUT_MS,
        probe.status,
        probe.tcp_connected,
        probe.http_status_line.unwrap_or_else(|| "없음".to_string()),
        probe.error.unwrap_or_else(|| "없음".to_string())
    )
}
