use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::{backend_state, layout as paths};
use crate::adapters::llama_cpp::backend as llama_backend;
use crate::adapters::llama_cpp::install as llama_install;
use crate::adapters::llama_cpp::stream as backend_stream;
use crate::adapters::process::backend as backend_process;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::backend::admission::{GenerationAdmission, GenerationRelease};
#[cfg(test)]
use crate::runtime_core::inference::backend::lifecycle::parse_generation_record;
use crate::runtime_core::inference::backend::lifecycle::{
    render_generation_record, BackendGenerationRecord, BackendGenerationTerminalRecord,
    BackendSidecarRecord,
};
use crate::runtime_core::inference::backend::BackendAdapter;
use crate::runtime_core::inference::backend::{
    BackendChatRun, BackendChatSampling, MAX_CHAT_TIMEOUT_MS,
};
use crate::runtime_core::inference::model::manifest::quantization_for_artifact_hash;
use crate::runtime_core::inference::{
    resource,
    stream::{StreamOutcome, StreamTermination},
};
use crate::runtime_core::reporting::korean_guard;
use crate::state;
use llama_backend::{LlamaCppAdapter, ENV_BACKEND_PATH, LLAMA_CPP_BACKEND_ID};
#[cfg(test)]
use llama_backend::{DEFAULT_HOST, DEFAULT_PORT, ENV_BACKEND_PORT};
use llama_install::{
    install_blockers as backend_install_blockers,
    selected_release_artifact as selected_backend_release_artifact, ArchiveDownloadStatus,
    BackendReleaseArtifact, LLAMA_CPP_RELEASE,
};
#[cfg(test)]
use llama_install::{release_artifact_for, BackendArchiveKind};

const HEALTH_TIMEOUT_MS: u64 = 500;
const ENV_BACKEND_START_TRACE: &str = "RPOTATO_TEST_BACKEND_START_TRACE";
const STARTUP_TIMEOUT_MS: u64 = 60_000;
const STOP_TIMEOUT_MS: u64 = 5_000;
const CHAT_TIMEOUT_MS: u64 = 30_000;
const CANCEL_WAIT_MS: u64 = 2_000;
const STOP_CANCEL_WAIT_MS: u64 = 5_000;
const TERMINAL_RECORD_RETENTION_MS: u128 = 5 * 60 * 1_000;
const DEFAULT_CHAT_MAX_TOKENS: u32 = 128;
const CHAT_SAMPLING: BackendChatSampling = BackendChatSampling {
    temperature: 0.1,
    top_p: 0.8,
};
const QWEN_NON_THINKING_SOURCE: &str =
    "https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode";
static GENERATION_ADMISSION_STATE: Mutex<GenerationAdmission> =
    Mutex::new(GenerationAdmission::new());
static BACKEND_RESOURCE_SAMPLE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendInstallResult {
    download_status: ArchiveDownloadStatus,
    archive_path: PathBuf,
    extracted_binary: PathBuf,
    managed_binary: PathBuf,
    binary_sha256: String,
    ledger_event: String,
}

struct GenerationTerminalContext {
    started_event: String,
    started_at_ms: u128,
    elapsed_ms: u128,
    requested_max_tokens: u32,
    effective_max_tokens: u32,
}

struct ActiveGenerationGuard {
    generation_id: String,
    finished: bool,
}

impl Drop for ActiveGenerationGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = release_generation_admission(&self.generation_id);
        }
    }
}

impl ActiveGenerationGuard {
    fn finish(mut self) -> Result<(), AppError> {
        release_generation_admission(&self.generation_id)?;
        self.finished = true;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct BackendResourceSampleReport {
    metric: observability::ResourceSampleMetric,
    ledger_event: String,
    pressure: resource::ResourcePressure,
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

pub fn install_plan_report() -> String {
    let discovery = llama_backend::discover();
    let artifact = selected_backend_release_artifact(&LLAMA_CPP_RELEASE);
    let blockers = backend_install_blockers(&LLAMA_CPP_RELEASE, artifact);
    let install_status = if blockers.is_empty() {
        "ready"
    } else {
        "blocked"
    };
    let archive_name = artifact
        .map(|artifact| artifact.archive_name)
        .unwrap_or("미확정");
    let download_path = paths::downloads_dir().join(if archive_name == "미확정" {
        "llama.cpp.archive.part"
    } else {
        archive_name
    });

    format!(
        "backend install plan\n- id: {}\n- status: {}\n- upstream source: {}\n- license: {}\n- license source: {}\n- license checked-at: {}\n- release tag: {}\n- release URL: {}\n- release API source: {}\n- release checked-at: {}\n- platform: {}/{}\n- archive URL: {}\n- archive name: {}\n- archive kind: {}\n- archive size bytes: {}\n- archive sha256: {}\n- binary in archive: {}\n- managed binary: {}\n- selected binary: {}\n- selected source: {}\n- download path: {}\n- blockers: {}\n- 동작: 실제 backend 다운로드 전 release URL, checksum, size, license를 사용자에게 표시해야 합니다.",
        LLAMA_CPP_RELEASE.id,
        install_status,
        LLAMA_CPP_RELEASE.upstream_source,
        LLAMA_CPP_RELEASE.license,
        LLAMA_CPP_RELEASE.license_source,
        LLAMA_CPP_RELEASE.license_checked_at,
        LLAMA_CPP_RELEASE.release_tag,
        LLAMA_CPP_RELEASE.release_url,
        LLAMA_CPP_RELEASE.release_api_source,
        LLAMA_CPP_RELEASE.release_checked_at,
        env::consts::OS,
        env::consts::ARCH,
        artifact
            .map(|artifact| artifact.archive_url)
            .unwrap_or("미확정"),
        archive_name,
        artifact
            .map(|artifact| artifact.archive_kind.as_str())
            .unwrap_or("미확정"),
        artifact
            .map(|artifact| artifact.archive_size_bytes)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        artifact
            .map(|artifact| artifact.archive_sha256)
            .unwrap_or("미확정"),
        artifact
            .map(|artifact| artifact.binary_relative_path)
            .unwrap_or("미확정"),
        discovery.managed_path.display(),
        discovery.selected_path.display(),
        discovery.selected_source,
        download_path.display(),
        display_vec(&blockers)
    )
}

pub fn install_report() -> Result<String, AppError> {
    let artifact = selected_backend_release_artifact(&LLAMA_CPP_RELEASE).ok_or_else(|| {
        AppError::blocked(format!(
            "backend install 차단\n- 이유: 지원 platform artifact 미확정 ({}/{})\n- 다음 단계: backend install-plan으로 현재 platform 상태를 확인하세요.",
            env::consts::OS,
            env::consts::ARCH
        ))
    })?;
    let blockers = backend_install_blockers(&LLAMA_CPP_RELEASE, Some(artifact));
    if !blockers.is_empty() {
        return Err(AppError::blocked(format!(
            "backend install 차단\n- blockers: {}\n- 다음 단계: backend install-plan으로 release URL, checksum, size, license source를 확인하세요.",
            display_vec(&blockers)
        )));
    }

    let archive_path = llama_install::archive_path(artifact);
    let download_status = llama_install::download_archive(artifact, &archive_path)?;
    llama_install::verify_archive_file(artifact, &archive_path)?;

    let managed_binary = LlamaCppAdapter.managed_binary_path();
    let staging_dir = llama_install::staging_dir(&LLAMA_CPP_RELEASE, artifact);
    let result = install_backend_from_archive(
        artifact,
        &archive_path,
        &managed_binary,
        &staging_dir,
        download_status,
    )?;

    Ok(format!(
        "backend install 완료\n- id: {}\n- release tag: {}\n- archive: {}\n- archive sha256: {}\n- archive source: {}\n- download status: {}\n- extracted binary: {}\n- managed binary: {}\n- managed binary sha256: {}\n- ledger event: {}\n- 다음 단계: rpotato backend doctor 또는 rpotato backend health-check로 상태를 확인하세요.",
        LLAMA_CPP_RELEASE.id,
        LLAMA_CPP_RELEASE.release_tag,
        result.archive_path.display(),
        artifact.archive_sha256,
        artifact.archive_url,
        result.download_status.as_str(),
        result.extracted_binary.display(),
        result.managed_binary.display(),
        result.binary_sha256,
        result.ledger_event
    ))
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
        "backend status\n- status: {}\n- backend: {}\n- pid: {}\n- binary: {}\n- model: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- health: {}\n- health error: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- stdout log: {}\n- stderr log: {}\n- sidecar record: {}",
        status,
        record.backend_id,
        record.pid,
        record.binary_path.display(),
        record.model_path.display(),
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

fn terminate_with_fallback(
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

fn cancel_active_generation_before_stop(record: &BackendSidecarRecord) -> Result<String, AppError> {
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

pub fn verify_archive_report(path: &str, expected_sha256: &str) -> Result<String, AppError> {
    if !checksum::is_valid_sha256(expected_sha256) {
        return Err(AppError::usage(
            "expected SHA-256은 64자리 hex string이어야 합니다.",
        ));
    }

    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(AppError::usage(format!(
            "검증 대상 backend archive를 찾지 못했습니다: {}",
            path.display()
        )));
    }

    let actual_sha256 = checksum::sha256_file(&path)?;
    let matched = actual_sha256.eq_ignore_ascii_case(expected_sha256);
    let event_id = state::record_event(
        if matched {
            "backend.archive.sha256.verified"
        } else {
            "backend.archive.sha256.rejected"
        },
        if matched {
            "backend archive SHA-256 검증 성공"
        } else {
            "backend archive SHA-256 검증 실패"
        },
        &format!(
            "path={} expected_sha256={} actual_sha256={}",
            path.display(),
            expected_sha256,
            actual_sha256
        ),
    )?;

    if !matched {
        return Err(AppError::blocked(format!(
            "backend archive SHA-256 검증 실패\n- path: {}\n- expected: {}\n- actual: {}\n- ledger event: {}\n- 동작: backend install과 extraction을 차단해야 합니다.",
            path.display(),
            expected_sha256,
            actual_sha256,
            event_id
        )));
    }

    Ok(format!(
        "backend archive SHA-256 검증 성공\n- path: {}\n- expected: {}\n- actual: {}\n- ledger event: {}",
        path.display(),
        expected_sha256,
        actual_sha256,
        event_id
    ))
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

pub fn chat_report(
    prompt: &str,
    max_tokens: Option<u32>,
    timeout_ms: Option<u32>,
) -> Result<String, AppError> {
    let run = chat_once_with_options(
        prompt,
        max_tokens,
        false,
        timeout_ms,
        || Ok(false),
        |_| Ok(()),
    )?;

    Ok(format_chat_run(&run, true))
}

pub fn chat_stream_report(
    prompt: &str,
    max_tokens: Option<u32>,
    timeout_ms: Option<u32>,
    writer: &mut impl Write,
) -> Result<String, AppError> {
    let mut language_guard = korean_guard::StreamingGuard::default();
    writer
        .write_all(b"backend chat\n- status: streaming\n- response:\n")
        .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))?;
    writer
        .flush()
        .map_err(|err| AppError::runtime(format!("streaming output flush 실패: {err}")))?;
    let run = chat_once_with_options(
        prompt,
        max_tokens,
        true,
        timeout_ms,
        || Ok(false),
        |delta| {
            let guarded = match delta {
                Some(delta) => language_guard.push(delta),
                None => language_guard.finish(),
            }
            .map_err(AppError::blocked)?;
            if guarded.is_empty() {
                return Ok(());
            }
            writer
                .write_all(guarded.as_bytes())
                .and_then(|_| writer.flush())
                .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))
        },
    )?;
    writer
        .write_all(b"\n")
        .map_err(|err| AppError::runtime(format!("streaming output write 실패: {err}")))?;

    Ok(format_chat_run(&run, false))
}

fn format_chat_run(run: &BackendChatRun, include_response: bool) -> String {
    let mut report = format!(
        "backend chat{}\n- status: completed\n- backend: {}\n- pid: {}\n- endpoint: /v1/chat/completions\n- transport: server-sent events\n- streaming display: {}\n- thinking mode: disabled via chat_template_kwargs.enable_thinking=false\n- non-thinking source: {}\n- model id: {}\n- model path: {}\n- ctx size: {}\n- prompt chars: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- resource governor admission: {}\n- resource governor token action: {}\n- resource governor reason: {}\n- resource governor hint: {}\n- resource governor sample event: {}\n- finish reason: {}\n- guard: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- first token latency ms: {}\n- elapsed ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- ledger event: {}",
        if include_response { "" } else { " summary" },
        run.backend_id,
        run.pid,
        run.streaming_display,
        QWEN_NON_THINKING_SOURCE,
        run.model_id,
        run.model_path.display(),
        display_optional_u32(run.ctx_size),
        run.prompt_chars,
        run.requested_max_tokens,
        run.effective_max_tokens,
        run.resource_governor_admission,
        run.resource_governor_token_action,
        run.resource_governor_reason,
        run.resource_governor_hint,
        run.resource_governor_sample_event,
        run.finish_reason,
        run.guard_status,
        display_optional_u32(run.prompt_tokens),
        display_optional_u32(run.completion_tokens),
        display_optional_u32(run.total_tokens),
        display_optional_u128(run.first_token_latency_ms),
        run.elapsed_ms,
        run.resource_pressure,
        display_optional_f64(run.resource_cpu_percent),
        display_optional_u64_unknown(run.resource_average_rss_bytes),
        display_optional_u64_unknown(run.resource_peak_rss_bytes),
        display_optional_u64_unknown(run.resource_disk_bytes),
        run.resource_sample_event,
        run.ledger_event
    );
    if include_response {
        report.push_str("\n- response:\n");
        report.push_str(&run.response);
    }
    report
}

pub fn chat_once(prompt: &str, max_tokens: Option<u32>) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(prompt, max_tokens, false, None, || Ok(false), |_| Ok(()))
}

pub fn chat_once_bounded(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(
        prompt,
        Some(max_tokens),
        false,
        Some(timeout_ms),
        || Ok(false),
        |_| Ok(()),
    )
}

pub fn chat_once_bounded_with_cancel(
    prompt: &str,
    max_tokens: u32,
    timeout_ms: u32,
    cancel_requested: impl FnMut() -> Result<bool, AppError>,
) -> Result<BackendChatRun, AppError> {
    chat_once_with_options(
        prompt,
        Some(max_tokens),
        false,
        Some(timeout_ms),
        cancel_requested,
        |_| Ok(()),
    )
}

pub fn preflight_chat_ready() -> Result<(), AppError> {
    ready_sidecar_record().map(|_| ())
}

fn ready_sidecar_record() -> Result<BackendSidecarRecord, AppError> {
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

fn chat_once_with_options(
    prompt: &str,
    max_tokens: Option<u32>,
    streaming_display: bool,
    timeout_ms: Option<u32>,
    mut external_cancel_requested: impl FnMut() -> Result<bool, AppError>,
    mut on_delta: impl FnMut(Option<&str>) -> Result<(), AppError>,
) -> Result<BackendChatRun, AppError> {
    if prompt.trim().is_empty() {
        return Err(AppError::usage(
            "backend chat은 비어 있지 않은 --prompt <text> 값이 필요합니다.",
        ));
    }
    let requested_max_tokens = max_tokens.unwrap_or(DEFAULT_CHAT_MAX_TOKENS);
    let record = ready_sidecar_record()?;

    let governor_sample = record_backend_resource_sample(&record, "chat-governor")?;
    let governor = resource::chat_governor_decision(governor_sample.pressure, requested_max_tokens);
    if governor.is_blocked() {
        let event_id = state::record_event(
            "backend.chat.governor.blocked",
            "backend chat resource governor 차단",
            &format!(
                "pid={} backend={} prompt_chars={} requested_max_tokens={} pressure_status={} admission={} token_action={} reason={} sample_event={}",
                record.pid,
                record.backend_id,
                prompt.chars().count(),
                requested_max_tokens,
                governor.pressure.as_str(),
                governor.admission.as_str(),
                governor.token_action.as_str(),
                governor.reason,
                governor_sample.ledger_event
            ),
        )?;
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: resource governor가 critical pressure에서 요청을 차단했습니다.\n- pid: {}\n- resource pressure: {}\n- requested max tokens: {}\n- effective max tokens: blocked\n- resource governor admission: {}\n- resource governor token action: {}\n- resource governor reason: {}\n- resource governor hint: {}\n- resource governor sample event: {}\n- ledger event: {}",
            record.pid,
            governor.pressure.as_str(),
            requested_max_tokens,
            governor.admission.as_str(),
            governor.token_action.as_str(),
            governor.reason,
            governor.hint,
            governor_sample.ledger_event,
            event_id
        )));
    }
    let effective_max_tokens = governor
        .effective_max_tokens
        .unwrap_or(requested_max_tokens);

    let timeout_ms = timeout_ms.unwrap_or(CHAT_TIMEOUT_MS as u32);
    if timeout_ms == 0 || timeout_ms > MAX_CHAT_TIMEOUT_MS {
        return Err(AppError::usage(format!(
            "backend chat timeout은 1..={MAX_CHAT_TIMEOUT_MS} ms 범위여야 합니다."
        )));
    }
    let generation = begin_active_generation(&record, timeout_ms, streaming_display)?;
    let generation_guard = ActiveGenerationGuard {
        generation_id: generation.generation_id.clone(),
        finished: false,
    };
    let started_event = state::record_event(
        "backend.generation.started",
        "backend generation 시작",
        &format!(
            "generation_id={} client_pid={} sidecar_pid={} backend={} model_id={} prompt_chars={} requested_max_tokens={} effective_max_tokens={} timeout_ms={} transport=sse streaming_display={} resource_governor_sample_event={}",
            generation.generation_id,
            generation.client_pid,
            generation.sidecar_pid,
            record.backend_id,
            model_id_from_path(&record.model_path),
            prompt.chars().count(),
            requested_max_tokens,
            effective_max_tokens,
            timeout_ms,
            streaming_display,
            governor_sample.ledger_event
        ),
    )?;
    let started_at_ms = now_ms();
    let started_at = Instant::now();
    let sampling = CHAT_SAMPLING;
    let body = llama_backend::chat_request_body(
        &record.model_path,
        prompt,
        effective_max_tokens,
        &sampling,
        true,
    );
    let stream_outcome = backend_stream::post_chat_stream(
        &record.host,
        record.port,
        "/v1/chat/completions",
        &body,
        Duration::from_millis(u64::from(timeout_ms)),
        || {
            if generation_cancel_requested(&generation.generation_id)? {
                return Ok(true);
            }
            external_cancel_requested()
        },
        |delta| on_delta(Some(delta)),
    );
    let stream_outcome = match stream_outcome {
        Ok(outcome) if outcome.termination == StreamTermination::Completed => {
            on_delta(None).map(|()| outcome)
        }
        other => other,
    };
    let elapsed_ms = started_at.elapsed().as_millis();
    let outcome = match stream_outcome {
        Ok(outcome) => outcome,
        Err(err) => {
            trace_backend_start(&format!(
                "generation-failed code={} message={}",
                err.code,
                err.message.replace('\n', " | ")
            ));
            let event_id = state::record_event(
                "backend.generation.failed",
                "backend generation 실패",
                &format!(
                    "generation_id={} sidecar_pid={} started_event={} timeout_ms={} elapsed_ms={} error_code={} error_detail=redacted",
                    generation.generation_id,
                    record.pid,
                    started_event,
                    timeout_ms,
                    elapsed_ms, err.code
                ),
            )?;
            write_generation_terminal_record(&generation.generation_id, "failed", &event_id)?;
            let resource_sample = record_backend_resource_sample(&record, "chat-failed")?;
            let identity = ledger::validated_current_identity()?;
            observability::record_model_run(&observability::ModelRunMetric {
                model_run_id: format!("model-run-{event_id}"),
                session_id: identity.session_id,
                workflow_id: None,
                model_id: model_id_from_path(&record.model_path),
                model_artifact_hash: Some(record.model_sha256.clone()),
                backend_id: Some(record.backend_id.clone()),
                backend_version: Some(record.backend_release.clone()),
                quantization: quantization_for_artifact_hash(&record.model_sha256)
                    .map(str::to_string),
                context_limit_tokens: record.ctx_size,
                started_at_ms,
                first_token_latency_ms: None,
                total_latency_ms: Some(elapsed_ms as f64),
                prompt_eval_ms: None,
                generation_eval_ms: None,
                tokens_per_second: None,
                cancelled: false,
                token_usage_complete: false,
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                context_tokens_used: 0,
                context_tokens_dropped: 0,
                ontology_tokens: 0,
                tool_summary_tokens: 0,
                max_output_tokens: Some(effective_max_tokens),
            })?;
            generation_guard.finish()?;
            return Err(AppError {
                code: err.code,
                message: format!(
                    "{}\n- resource sample event: {}\n- lifecycle event: {event_id}",
                    err.message, resource_sample.ledger_event
                ),
            });
        }
    };
    if outcome.termination != StreamTermination::Completed {
        let interrupted = finish_interrupted_generation(
            &record,
            &generation,
            &outcome,
            GenerationTerminalContext {
                started_event,
                started_at_ms,
                elapsed_ms,
                requested_max_tokens,
                effective_max_tokens,
            },
        );
        generation_guard.finish()?;
        return interrupted;
    }

    let completion = outcome.completion;
    let display_content = completion.content.trim().to_string();
    let guard_status = if completion.had_reasoning_trace {
        if display_content.is_empty() {
            "blocked-empty-after-reasoning-strip"
        } else {
            "stripped-reasoning-trace"
        }
    } else {
        "pass"
    };
    let event_type = if display_content.is_empty() {
        "backend.chat.guard.blocked"
    } else {
        "backend.chat.completed"
    };
    let event_id = state::record_event(
        event_type,
        "backend chat completion 실행",
        &format!(
            "generation_id={} started_event={} pid={} backend={} backend_release={} binary_sha256={} model_id={} model_sha256={} model_size_bytes={} ctx_size={} mmproj={} sampling={} host_os={} host_arch={} prompt_chars={} output_chars={} requested_max_tokens={} effective_max_tokens={} timeout_ms={} transport=sse streaming_display={} resource_governor_admission={} resource_governor_token_action={} resource_governor_reason={} resource_governor_sample_event={} finish_reason={} guard_status={} prompt_tokens={} completion_tokens={} total_tokens={} first_token_latency_ms={} elapsed_ms={}",
            generation.generation_id,
            started_event,
            record.pid,
            record.backend_id,
            record.backend_release,
            record.binary_sha256,
            model_id_from_path(&record.model_path),
            record.model_sha256,
            record.model_size_bytes,
            display_optional_u32(record.ctx_size),
            record.mmproj,
            sampling.ledger_label(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            prompt.chars().count(),
            display_content.chars().count(),
            requested_max_tokens,
            effective_max_tokens,
            timeout_ms,
            streaming_display,
            governor.admission.as_str(),
            governor.token_action.as_str(),
            governor.reason,
            governor_sample.ledger_event,
            completion.finish_reason,
            guard_status,
            display_optional_u32(completion.prompt_tokens),
            display_optional_u32(completion.completion_tokens),
            display_optional_u32(completion.total_tokens),
            display_optional_u128(completion.first_token_latency_ms),
            elapsed_ms
        ),
    )?;

    write_generation_terminal_record(&generation.generation_id, "completed", &event_id)?;
    let resource_sample = record_backend_resource_sample(
        &record,
        if streaming_display {
            "chat-stream"
        } else {
            "chat"
        },
    )?;

    let identity = ledger::validated_current_identity()?;
    let model_id = model_id_from_path(&record.model_path);
    let model_run_id = format!("model-run-{event_id}");
    let completion_tokens = completion.completion_tokens.unwrap_or(0);
    let tokens_per_second = if completion_tokens > 0 && elapsed_ms > 0 {
        Some((completion_tokens as f64) / ((elapsed_ms as f64) / 1000.0))
    } else {
        None
    };
    observability::record_model_run(&observability::ModelRunMetric {
        model_run_id,
        session_id: identity.session_id,
        workflow_id: None,
        model_id: model_id.clone(),
        model_artifact_hash: Some(record.model_sha256.clone()),
        backend_id: Some(record.backend_id.clone()),
        backend_version: Some(record.backend_release.clone()),
        quantization: quantization_for_artifact_hash(&record.model_sha256).map(str::to_string),
        context_limit_tokens: record.ctx_size,
        started_at_ms,
        first_token_latency_ms: completion.first_token_latency_ms.map(|value| value as f64),
        total_latency_ms: Some(elapsed_ms as f64),
        prompt_eval_ms: None,
        generation_eval_ms: None,
        tokens_per_second,
        cancelled: false,
        token_usage_complete: completion.prompt_tokens.is_some()
            && completion.completion_tokens.is_some()
            && completion.total_tokens.is_some(),
        prompt_tokens: completion.prompt_tokens.unwrap_or(0),
        completion_tokens,
        total_tokens: completion.total_tokens.unwrap_or(0),
        context_tokens_used: completion.prompt_tokens.unwrap_or(0),
        context_tokens_dropped: 0,
        ontology_tokens: 0,
        tool_summary_tokens: 0,
        max_output_tokens: Some(effective_max_tokens),
    })?;
    if display_content.is_empty() {
        generation_guard.finish()?;
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: reasoning trace 제거 후 표시 가능한 응답이 없습니다.\n- endpoint: /v1/chat/completions\n- thinking mode: disabled via chat_template_kwargs.enable_thinking=false\n- guard: {}\n- finish reason: {}\n- resource sample event: {}\n- lifecycle event: {}",
            guard_status,
            completion.finish_reason,
            resource_sample.ledger_event,
            event_id
        )));
    }

    let run = BackendChatRun {
        backend_id: record.backend_id,
        backend_version: record.backend_release,
        pid: record.pid,
        model_id,
        model_path: record.model_path,
        model_artifact_hash: record.model_sha256,
        ctx_size: record.ctx_size,
        prompt_chars: prompt.chars().count(),
        response_chars: display_content.chars().count(),
        requested_max_tokens,
        effective_max_tokens,
        sampling,
        finish_reason: completion.finish_reason,
        guard_status,
        prompt_tokens: completion.prompt_tokens,
        completion_tokens: completion.completion_tokens,
        total_tokens: completion.total_tokens,
        elapsed_ms,
        first_token_latency_ms: completion.first_token_latency_ms,
        streaming_display,
        ledger_event: event_id,
        resource_governor_admission: governor.admission.as_str().to_string(),
        resource_governor_token_action: governor.token_action.as_str().to_string(),
        resource_governor_reason: governor.reason,
        resource_governor_hint: governor.hint,
        resource_governor_sample_event: governor_sample.ledger_event,
        resource_pressure: resource_sample.metric.pressure_status,
        resource_cpu_percent: resource_sample.metric.process_cpu_percent,
        resource_average_rss_bytes: resource_sample.metric.average_rss_bytes,
        resource_peak_rss_bytes: resource_sample.metric.peak_rss_bytes,
        resource_disk_bytes: resource_sample.metric.disk_bytes,
        resource_sample_event: resource_sample.ledger_event,
        response: display_content,
    };
    generation_guard.finish()?;
    Ok(run)
}

fn finish_interrupted_generation(
    record: &BackendSidecarRecord,
    generation: &BackendGenerationRecord,
    outcome: &StreamOutcome,
    terminal: GenerationTerminalContext,
) -> Result<BackendChatRun, AppError> {
    let (event_type, status, status_label, resource_label) = match outcome.termination {
        StreamTermination::Cancelled => (
            "backend.generation.cancelled",
            "cancelled",
            "사용자 요청으로 취소됨",
            "chat-cancelled",
        ),
        StreamTermination::TimedOut => (
            "backend.generation.timeout",
            "timed-out",
            "제한 시간 초과로 취소됨",
            "chat-timeout",
        ),
        StreamTermination::Completed => {
            return Err(AppError::runtime(
                "완료된 generation을 interrupted 상태로 처리할 수 없습니다.",
            ));
        }
    };
    let completion = &outcome.completion;
    let event_id = state::record_event(
        event_type,
        "backend generation 중단",
        &format!(
            "generation_id={} started_event={} client_pid={} sidecar_pid={} status={} timeout_ms={} elapsed_ms={} output_chars={} requested_max_tokens={} effective_max_tokens={} first_token_latency_ms={} prompt_tokens={} completion_tokens={} total_tokens={}",
            generation.generation_id,
            terminal.started_event,
            generation.client_pid,
            generation.sidecar_pid,
            status,
            generation.timeout_ms,
            terminal.elapsed_ms,
            completion.content.chars().count(),
            terminal.requested_max_tokens,
            terminal.effective_max_tokens,
            display_optional_u128(completion.first_token_latency_ms),
            display_optional_u32(completion.prompt_tokens),
            display_optional_u32(completion.completion_tokens),
            display_optional_u32(completion.total_tokens)
        ),
    )?;
    write_generation_terminal_record(&generation.generation_id, status, &event_id)?;
    let resource_sample = record_backend_resource_sample(record, resource_label)?;
    let identity = ledger::validated_current_identity()?;
    observability::record_model_run(&observability::ModelRunMetric {
        model_run_id: format!("model-run-{event_id}"),
        session_id: identity.session_id,
        workflow_id: None,
        model_id: model_id_from_path(&record.model_path),
        model_artifact_hash: Some(record.model_sha256.clone()),
        backend_id: Some(record.backend_id.clone()),
        backend_version: Some(record.backend_release.clone()),
        quantization: quantization_for_artifact_hash(&record.model_sha256).map(str::to_string),
        context_limit_tokens: record.ctx_size,
        started_at_ms: terminal.started_at_ms,
        first_token_latency_ms: completion.first_token_latency_ms.map(|value| value as f64),
        total_latency_ms: Some(terminal.elapsed_ms as f64),
        prompt_eval_ms: None,
        generation_eval_ms: None,
        tokens_per_second: None,
        cancelled: true,
        token_usage_complete: completion.prompt_tokens.is_some()
            && completion.completion_tokens.is_some()
            && completion.total_tokens.is_some(),
        prompt_tokens: completion.prompt_tokens.unwrap_or(0),
        completion_tokens: completion.completion_tokens.unwrap_or(0),
        total_tokens: completion.total_tokens.unwrap_or(0),
        context_tokens_used: completion.prompt_tokens.unwrap_or(0),
        context_tokens_dropped: 0,
        ontology_tokens: 0,
        tool_summary_tokens: 0,
        max_output_tokens: Some(terminal.effective_max_tokens),
    })?;
    Err(AppError::runtime(format!(
        "backend chat 중단\n- 상태: {status_label}\n- generation id: {}\n- sidecar pid: {}\n- 경과 시간 ms: {}\n- 부분 출력 문자 수: {}\n- resource sample event: {}\n- lifecycle event: {}\n- sidecar 동작: 계속 실행",
        generation.generation_id,
        generation.sidecar_pid,
        terminal.elapsed_ms,
        completion.content.chars().count(),
        resource_sample.ledger_event,
        event_id
    )))
}

pub fn cancel_generation_report() -> Result<String, AppError> {
    let Some(record) = backend_state::read_generation_record()? else {
        return Ok(format!(
            "backend generation 취소\n- status: idle\n- active generation record: {}",
            backend_state::generation_record_path().display()
        ));
    };
    if !backend_process::is_running(record.client_pid) {
        remove_generation_state_if_owned(&record.generation_id);
        let event_id = state::record_event(
            "backend.generation.stale.cleaned",
            "stale backend generation record 정리",
            &format!(
                "generation_id={} client_pid={} sidecar_pid={} reason=client-not-running",
                record.generation_id, record.client_pid, record.sidecar_pid
            ),
        )?;
        return Ok(format!(
            "backend generation 취소\n- status: stale-record-cleaned\n- generation id: {}\n- client pid: {}\n- sidecar pid: {}\n- sidecar action: kept-running\n- ledger event: {}",
            record.generation_id, record.client_pid, record.sidecar_pid, event_id
        ));
    }

    write_generation_cancel_marker(&record.generation_id)?;
    let event_id = state::record_event(
        "backend.generation.cancel.requested",
        "backend generation 취소 요청",
        &format!(
            "generation_id={} client_pid={} sidecar_pid={} transport=cancel-marker sidecar_action=kept-running",
            record.generation_id, record.client_pid, record.sidecar_pid
        ),
    )?;

    let wait_started = Instant::now();
    let terminal =
        wait_for_generation_terminal(&record.generation_id, Duration::from_millis(CANCEL_WAIT_MS))?;
    let remaining = Duration::from_millis(CANCEL_WAIT_MS).saturating_sub(wait_started.elapsed());
    let group_released = if terminal.is_some() {
        wait_for_generation_group_release(&record.generation_id, remaining)?
    } else {
        false
    };
    if group_released {
        backend_state::remove_generation_terminal_record(&record.generation_id)?;
    }
    let terminal_outcome = terminal
        .as_ref()
        .map(|record| record.outcome.as_str())
        .unwrap_or("pending");
    let terminal_event = terminal
        .as_ref()
        .map(|record| record.lifecycle_event.as_str())
        .unwrap_or("not-acknowledged");

    Ok(format!(
        "backend generation 취소\n- status: {}\n- terminal outcome: {}\n- generation id: {}\n- client pid: {}\n- sidecar pid: {}\n- wait ms: {}\n- sidecar action: kept-running\n- terminal lifecycle event: {}\n- request ledger event: {}",
        if terminal.is_some() && group_released { "acknowledged" } else { "requested" },
        terminal_outcome,
        record.generation_id,
        record.client_pid,
        record.sidecar_pid,
        wait_started.elapsed().as_millis(),
        terminal_event,
        event_id
    ))
}

fn install_backend_from_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
    managed_binary: &Path,
    staging_dir: &Path,
    download_status: ArchiveDownloadStatus,
) -> Result<BackendInstallResult, AppError> {
    let payload =
        llama_install::prepare_install(artifact, archive_path, managed_binary, staging_dir)?;
    llama_install::write_install_record(artifact, &payload.binary_sha256)?;
    llama_install::cleanup_staging(staging_dir)?;

    let event_id = state::record_event(
        "backend.install.completed",
        "llama.cpp backend 설치 완료",
        &format!(
            "release_tag={} archive={} sha256={} managed_binary={} binary_sha256={} download_status={}",
            LLAMA_CPP_RELEASE.release_tag,
            archive_path.display(),
            artifact.archive_sha256,
            managed_binary.display(),
            payload.binary_sha256,
            download_status.as_str()
        ),
    )?;

    Ok(BackendInstallResult {
        download_status,
        archive_path: payload.archive_path,
        extracted_binary: payload.extracted_binary,
        managed_binary: payload.managed_binary,
        binary_sha256: payload.binary_sha256,
        ledger_event: event_id,
    })
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

fn begin_active_generation(
    sidecar: &BackendSidecarRecord,
    timeout_ms: u32,
    streaming_display: bool,
) -> Result<BackendGenerationRecord, AppError> {
    let mut admission = GENERATION_ADMISSION_STATE
        .lock()
        .map_err(|_| AppError::runtime("backend generation admission lock poisoned"))?;
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
    Ok(record)
}

fn write_backend_generation_record(record: &BackendGenerationRecord) -> Result<(), AppError> {
    let path = backend_state::generation_record_path();
    let contents = render_generation_record(record);
    state::atomic_replace_bytes(&path, contents.as_bytes())
}

fn generation_cancel_requested(generation_id: &str) -> Result<bool, AppError> {
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

fn write_generation_cancel_marker(generation_id: &str) -> Result<(), AppError> {
    let marker = format!(
        "generation_id={}\nrequested_at_ms={}\nrequester_pid={}\n",
        generation_id,
        now_ms(),
        std::process::id()
    );
    state::atomic_replace_bytes(&backend_state::generation_cancel_path(), marker.as_bytes())
}

fn write_generation_terminal_record(
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
    state::atomic_replace_bytes(
        &backend_state::generation_terminal_path(generation_id),
        contents.as_bytes(),
    )
}

fn wait_for_generation_terminal(
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

fn wait_for_generation_group_release(
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

fn remove_generation_state_if_owned(generation_id: &str) {
    let _ = backend_state::remove_generation_state_if_owned(generation_id);
}

fn release_generation_admission(generation_id: &str) -> Result<(), AppError> {
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

fn start_sidecar_with_timeout(
    model_path: &str,
    ctx_size: Option<u32>,
    timeout: Duration,
) -> Result<String, AppError> {
    let model_path = canonical_existing_file(model_path, "model")?;
    let discovery = llama_backend::discover();
    if !discovery.binary_exists || !discovery.binary_is_file {
        return Err(AppError::blocked(format!(
            "backend start 차단\n- 이유: backend binary를 찾지 못했습니다.\n- selected binary: {}\n- 다음 단계: rpotato backend install 또는 {} 설정",
            discovery.selected_path.display(),
            ENV_BACKEND_PATH
        )));
    }
    if !discovery.binary_executable {
        return Err(AppError::blocked(format!(
            "backend start 차단\n- 이유: backend binary 실행 권한이 없습니다.\n- selected binary: {}",
            discovery.selected_path.display()
        )));
    }

    if let Some(record) = backend_state::read_sidecar_record()? {
        if backend_process::is_running(record.pid) {
            let resource_sample = record_backend_resource_sample(&record, "start-existing")?;
            return Ok(format!(
                "backend start\n- status: already-running\n- pid: {}\n- binary: {}\n- model: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- stdout log: {}\n- stderr log: {}",
                record.pid,
                record.binary_path.display(),
                record.model_path.display(),
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
            ));
        }
        backend_state::remove_sidecar_record()?;
    }

    let binary_path = fs::canonicalize(&discovery.selected_path).map_err(|err| {
        AppError::runtime(format!(
            "backend binary canonical path 계산 실패: {} ({err})",
            discovery.selected_path.display()
        ))
    })?;
    let model_size_bytes = model_path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "model artifact metadata를 읽지 못했습니다: {} ({err})",
                model_path.display()
            ))
        })?
        .len();
    let model_sha256 = checksum::sha256_file(&model_path)?;
    let binary_sha256 = checksum::sha256_file(&binary_path)?;
    let backend_release = if discovery.selected_source == "managed" {
        LLAMA_CPP_RELEASE.release_tag.to_string()
    } else {
        "override-unverified".to_string()
    };
    fs::create_dir_all(paths::logs_dir()).map_err(|err| {
        AppError::runtime(format!(
            "backend log directory를 만들지 못했습니다: {} ({err})",
            paths::logs_dir().display()
        ))
    })?;
    let run_id = now_ms();
    let stdout_log = paths::logs_dir().join(format!("backend-llama.cpp-{run_id}-stdout.log"));
    let stderr_log = paths::logs_dir().join(format!("backend-llama.cpp-{run_id}-stderr.log"));
    let stdout_file = create_log_file(&stdout_log)?;
    let stderr_file = create_log_file(&stderr_log)?;
    trace_backend_start("logs-created");

    let mut command = Command::new(&binary_path);
    command
        .arg("--model")
        .arg(&model_path)
        .arg("--host")
        .arg(discovery.host)
        .arg("--port")
        .arg(discovery.port.to_string());
    if let Some(ctx_size) = ctx_size {
        command.arg("--ctx-size").arg(ctx_size.to_string());
    }
    backend_process::configure_child(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|err| {
            AppError::runtime(format!(
                "backend sidecar 시작 실패: {} ({err})",
                binary_path.display()
            ))
        })?;
    trace_backend_start(&format!("sidecar-spawned pid={}", child.id()));

    let record = BackendSidecarRecord {
        backend_id: discovery.adapter_id.to_string(),
        pid: child.id(),
        binary_path,
        model_path,
        model_sha256,
        model_size_bytes,
        backend_release,
        binary_sha256,
        mmproj: "not-required-text-only".to_string(),
        host: discovery.host.to_string(),
        port: discovery.port,
        ctx_size,
        stdout_log,
        stderr_log,
        started_at_ms: now_ms(),
    };
    backend_state::write_sidecar_record(&record)?;
    trace_backend_start("sidecar-record-written");

    let started_at = Instant::now();
    loop {
        trace_backend_start("health-probe-start");
        let health = llama_backend::probe_health(
            &record.host,
            record.port,
            Duration::from_millis(HEALTH_TIMEOUT_MS),
        );
        trace_backend_start(&format!("health-probe-finished status={}", health.status));
        if health.status == "healthy" {
            let startup_ms = started_at.elapsed().as_millis();
            let event_id = state::record_event(
                "backend.sidecar.start.completed",
                "backend sidecar 시작 완료",
                &format!(
                    "pid={} backend={} backend_release={} binary_sha256={} model_id={} model_sha256={} model_size_bytes={} port={} ctx_size={} mmproj={} sampling=temperature-0.1_top-p-0.8 host_os={} host_arch={} startup_ms={}",
                    record.pid,
                    record.backend_id,
                    record.backend_release,
                    record.binary_sha256,
                    model_id_from_path(&record.model_path),
                    record.model_sha256,
                    record.model_size_bytes,
                    record.port,
                    display_optional_u32(record.ctx_size),
                    record.mmproj,
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    startup_ms
                ),
            )?;
            trace_backend_start("start-event-recorded");
            let resource_sample = record_backend_resource_sample(&record, "start")?;
            trace_backend_start("resource-sample-recorded");
            return Ok(format!(
                "backend start\n- status: running\n- pid: {}\n- binary: {}\n- model: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- startup ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
                record.pid,
                record.binary_path.display(),
                record.model_path.display(),
                record.host,
                record.port,
                display_optional_u32(record.ctx_size),
                startup_ms,
                resource_sample.metric.pressure_status,
                display_optional_f64(resource_sample.metric.process_cpu_percent),
                display_optional_u64_unknown(resource_sample.metric.average_rss_bytes),
                display_optional_u64_unknown(resource_sample.metric.peak_rss_bytes),
                display_optional_u64_unknown(resource_sample.metric.disk_bytes),
                resource_sample.ledger_event,
                record.stdout_log.display(),
                record.stderr_log.display(),
                event_id
            ));
        }

        if let Some(status) = child.try_wait().map_err(|err| {
            AppError::runtime(format!("backend sidecar process 상태 확인 실패: {err}"))
        })? {
            backend_state::remove_sidecar_record()?;
            let event_id = state::record_event(
                "backend.sidecar.start.failed",
                "backend sidecar 시작 실패",
                &format!(
                    "pid={} exit_status={} stdout_log={} stderr_log={}",
                    record.pid,
                    status,
                    record.stdout_log.display(),
                    record.stderr_log.display()
                ),
            )?;
            return Err(AppError::blocked(format!(
                "backend start 실패\n- pid: {}\n- exit status: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
                record.pid,
                status,
                record.stdout_log.display(),
                record.stderr_log.display(),
                event_id
            )));
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            backend_state::remove_sidecar_record()?;
            let event_id = state::record_event(
                "backend.sidecar.start.timeout",
                "backend sidecar 시작 timeout",
                &format!(
                    "pid={} timeout_ms={} stdout_log={} stderr_log={}",
                    record.pid,
                    timeout.as_millis(),
                    record.stdout_log.display(),
                    record.stderr_log.display()
                ),
            )?;
            return Err(AppError::blocked(format!(
                "backend start timeout\n- pid: {}\n- timeout ms: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
                record.pid,
                timeout.as_millis(),
                record.stdout_log.display(),
                record.stderr_log.display(),
                event_id
            )));
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn trace_backend_start(message: &str) {
    let Some(path) = env::var_os(ENV_BACKEND_START_TRACE) else {
        return;
    };
    let Ok(mut trace) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(trace, "{message}");
    let _ = trace.flush();
}

fn canonical_existing_file(path: &str, label: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(AppError::usage(format!(
            "{label} file을 찾지 못했습니다: {}",
            path.display()
        )));
    }
    fs::canonicalize(&path).map_err(|err| {
        AppError::runtime(format!(
            "{label} file canonical path 계산 실패: {} ({err})",
            path.display()
        ))
    })
}

fn create_log_file(path: &Path) -> Result<File, AppError> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|err| AppError::runtime(format!("log file 생성 실패: {} ({err})", path.display())))
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
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn termination_fallback_forces_a_process_after_graceful_command_failure() {
        let calls = std::cell::RefCell::new(Vec::new());
        let running = std::cell::Cell::new(true);

        terminate_with_fallback(
            || {
                calls.borrow_mut().push("graceful");
                Err(AppError::runtime("graceful unsupported"))
            },
            || {
                calls.borrow_mut().push("force");
                running.set(false);
                Ok(())
            },
            || Ok(running.get()),
            || Ok(!running.get()),
            42,
        )
        .unwrap();

        assert_eq!(*calls.borrow(), ["graceful", "force"]);
        assert!(!running.get());
    }

    #[test]
    fn termination_fallback_accepts_force_race_when_process_is_already_gone() {
        let running = std::cell::Cell::new(true);

        terminate_with_fallback(
            || Err(AppError::runtime("graceful unsupported")),
            || {
                running.set(false);
                Err(AppError::runtime("process already exited"))
            },
            || Ok(running.get()),
            || Ok(!running.get()),
            43,
        )
        .unwrap();

        assert!(!running.get());
    }

    #[test]
    fn termination_fallback_fails_closed_when_liveness_check_fails() {
        let force_called = std::cell::Cell::new(false);

        let error = terminate_with_fallback(
            || Err(AppError::runtime("graceful unsupported")),
            || {
                force_called.set(true);
                Ok(())
            },
            || Err(AppError::runtime("liveness unavailable")),
            || Ok(false),
            44,
        )
        .unwrap_err();

        assert!(error.message.contains("liveness unavailable"));
        assert!(!force_called.get());
    }
    fn generation_test_sidecar() -> BackendSidecarRecord {
        BackendSidecarRecord {
            backend_id: LLAMA_CPP_BACKEND_ID.to_string(),
            pid: std::process::id(),
            binary_path: PathBuf::from("llama-server"),
            model_path: PathBuf::from("model.gguf"),
            model_sha256: "a".repeat(64),
            model_size_bytes: 1,
            backend_release: LLAMA_CPP_RELEASE.release_tag.to_string(),
            binary_sha256: "b".repeat(64),
            mmproj: "not-required-text-only".to_string(),
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            ctx_size: Some(4096),
            stdout_log: PathBuf::from("stdout.log"),
            stderr_log: PathBuf::from("stderr.log"),
            started_at_ms: now_ms(),
        }
    }

    #[test]
    fn default_discovery_uses_managed_path() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        env::remove_var(ENV_BACKEND_PATH);
        env::remove_var(ENV_BACKEND_PORT);
        let data_root =
            env::temp_dir().join(format!("rpotato-backend-test-{}", std::process::id()));
        env::set_var("RPOTATO_DATA_HOME", &data_root);

        let discovery = llama_backend::discover();

        env::remove_var("RPOTATO_DATA_HOME");
        assert_eq!(discovery.adapter_id, "llama.cpp");
        assert_eq!(discovery.selected_source, "managed");
        assert!(discovery
            .selected_path
            .ends_with(LlamaCppAdapter.binary_name()));
        assert_eq!(discovery.port, DEFAULT_PORT);
    }

    #[test]
    fn backend_path_and_port_can_come_from_env() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let override_path = env::temp_dir().join("custom-llama-server");
        env::set_var(ENV_BACKEND_PATH, &override_path);
        env::set_var(ENV_BACKEND_PORT, "19090");

        let discovery = llama_backend::discover();

        env::remove_var(ENV_BACKEND_PATH);
        env::remove_var(ENV_BACKEND_PORT);
        assert_eq!(discovery.selected_path, override_path);
        assert_eq!(discovery.selected_source, "env override");
        assert_eq!(discovery.port, 19090);
        assert_eq!(discovery.port_source, "env override");
    }

    #[test]
    fn invalid_backend_port_falls_back_to_default() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        env::set_var(ENV_BACKEND_PORT, "0");

        let discovery = llama_backend::discover();

        env::remove_var(ENV_BACKEND_PORT);
        assert_eq!(discovery.port, DEFAULT_PORT);
        assert_eq!(discovery.port_source, "invalid env, default");
    }

    #[test]
    fn release_manifest_has_source_backed_supported_artifacts() {
        let expected = [
            (
                "macos",
                "aarch64",
                "llama-b9982-bin-macos-arm64.tar.gz",
                "9606e3a609bc9483730f50f17ce78c3d764df8eaec63fcbb47d2f8b235667c9c",
                10_746_432,
                BackendArchiveKind::TarGz,
                "llama-server",
            ),
            (
                "macos",
                "x86_64",
                "llama-b9982-bin-macos-x64.tar.gz",
                "da109cc18574392ab88936de826ca00f8d196b9ef5a1c19da72fbfb06bea7cd0",
                11_022_427,
                BackendArchiveKind::TarGz,
                "llama-server",
            ),
            (
                "linux",
                "aarch64",
                "llama-b9982-bin-ubuntu-arm64.tar.gz",
                "9468c0282c15e286216a63122e7471f7d14888d3858bdab61b72d14a2531cf60",
                12_782_598,
                BackendArchiveKind::TarGz,
                "llama-server",
            ),
            (
                "linux",
                "x86_64",
                "llama-b9982-bin-ubuntu-x64.tar.gz",
                "0c1f0445f6f86a0f049de3586b7eabdde7108d827d0a9b2c5c0dc2185506ffee",
                15_850_588,
                BackendArchiveKind::TarGz,
                "llama-server",
            ),
            (
                "windows",
                "aarch64",
                "llama-b9982-bin-win-cpu-arm64.zip",
                "11ad20d8df121d5760900b4e2fa9943a065856075ef44df52ed7a8dc58b08b2f",
                12_151_247,
                BackendArchiveKind::Zip,
                "llama-server.exe",
            ),
            (
                "windows",
                "x86_64",
                "llama-b9982-bin-win-cpu-x64.zip",
                "69337038e8e56feb3c04d99588fa19f9241b294bae6f6c2e665a301605726e2a",
                18_247_652,
                BackendArchiveKind::Zip,
                "llama-server.exe",
            ),
        ];

        for (
            os,
            arch,
            archive_name,
            archive_sha256,
            archive_size_bytes,
            archive_kind,
            binary_relative_path,
        ) in expected
        {
            let artifact = release_artifact_for(&LLAMA_CPP_RELEASE, os, arch)
                .unwrap_or_else(|| panic!("{os}/{arch} backend artifact should be recorded"));
            assert_eq!(artifact.archive_name, archive_name);
            assert_eq!(
                artifact.archive_url,
                format!(
                    "https://github.com/ggml-org/llama.cpp/releases/download/{}/{}",
                    LLAMA_CPP_RELEASE.release_tag, artifact.archive_name
                )
            );
            assert_eq!(artifact.archive_sha256, archive_sha256);
            assert_eq!(artifact.archive_size_bytes, archive_size_bytes);
            assert_eq!(artifact.archive_kind, archive_kind);
            assert_eq!(artifact.binary_relative_path, binary_relative_path);
            assert_eq!(
                backend_install_blockers(&LLAMA_CPP_RELEASE, Some(artifact)),
                Vec::<String>::new()
            );
        }
    }

    #[test]
    fn install_plan_uses_current_platform_manifest_when_supported() {
        let report = install_plan_report();

        if selected_backend_release_artifact(&LLAMA_CPP_RELEASE).is_some() {
            assert!(report.contains("status: ready"));
            assert!(report.contains("archive sha256: "));
            assert!(report.contains(&format!("release tag: {}", LLAMA_CPP_RELEASE.release_tag)));
        } else {
            assert!(report.contains("status: blocked"));
            assert!(report.contains("지원 platform artifact 미확정"));
        }
    }

    #[test]
    fn release_artifact_selection_rejects_unknown_platform() {
        assert!(release_artifact_for(&LLAMA_CPP_RELEASE, "freebsd", "riscv64").is_none());
    }

    #[test]
    fn install_from_tar_archive_places_managed_payload() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-backend-install-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        let archive_path = root.join("backend.tar.gz");
        write_test_tar_gz(
            &archive_path,
            &[
                ("release/bin/llama-server", b"fake backend".as_slice()),
                ("release/bin/libllama.dylib", b"fake dylib".as_slice()),
            ],
        )
        .unwrap();

        let artifact = BackendReleaseArtifact {
            os: "test",
            arch: "test",
            archive_name: "backend.tar.gz",
            archive_url: "https://example.invalid/backend.tar.gz",
            archive_sha256: "0000000000000000000000000000000000000000000000000000000000000000",
            archive_size_bytes: archive_path.metadata().unwrap().len(),
            archive_kind: BackendArchiveKind::TarGz,
            binary_relative_path: "llama-server",
        };
        let managed_binary = root.join("managed").join("llama-server");
        let staging_dir = root.join("staging");

        let result = install_backend_from_archive(
            &artifact,
            &archive_path,
            &managed_binary,
            &staging_dir,
            ArchiveDownloadStatus::CacheHit,
        )
        .unwrap();

        assert!(managed_binary.is_file());
        assert!(llama_backend::is_executable(&managed_binary));
        assert_eq!(fs::read(&managed_binary).unwrap(), b"fake backend");
        assert_eq!(
            fs::read(managed_binary.parent().unwrap().join("libllama.dylib")).unwrap(),
            b"fake dylib"
        );
        assert_eq!(result.managed_binary, managed_binary);
        assert!(!staging_dir.exists());
        env::remove_var("RPOTATO_DATA_HOME");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn doctor_skips_version_for_env_override_binary() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        env::set_var(ENV_BACKEND_PATH, "/tmp/user-owned-llama-server");

        let report = doctor_report();

        env::remove_var(ENV_BACKEND_PATH);
        assert!(report.contains("version detection: skipped"));
        assert!(report.contains("env override backend binary"));
    }

    #[cfg(unix)]
    #[test]
    fn doctor_runs_version_for_recorded_managed_binary() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-backend-version-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        env::set_var("RPOTATO_DATA_HOME", &root);

        let artifact = selected_backend_release_artifact(&LLAMA_CPP_RELEASE).unwrap();
        let managed_binary = LlamaCppAdapter.managed_binary_path();
        fs::create_dir_all(managed_binary.parent().unwrap()).unwrap();
        let expected_version = format!("llama.cpp fake version {}", LLAMA_CPP_RELEASE.release_tag);
        fs::write(
            &managed_binary,
            format!("#!/bin/sh\necho '{expected_version}'\n"),
        )
        .unwrap();
        llama_install::set_executable_bit(&managed_binary).unwrap();
        let binary_sha256 = checksum::sha256_file(&managed_binary).unwrap();
        llama_install::write_install_record(artifact, &binary_sha256).unwrap();

        let report = doctor_report();

        env::remove_var("RPOTATO_DATA_HOME");
        fs::remove_dir_all(root).unwrap();
        assert!(report.contains("version detection: ok"));
        assert!(report.contains(&expected_version));
    }

    #[test]
    fn backend_status_reports_stopped_without_record() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-backend-status-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));

        let report = status_report().unwrap();

        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        fs::remove_dir_all(root).unwrap();
        assert!(report.contains("status: stopped"));
    }

    #[test]
    fn sidecar_record_round_trip_preserves_ctx_size() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-backend-record-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));

        let record = BackendSidecarRecord {
            backend_id: LLAMA_CPP_BACKEND_ID.to_string(),
            pid: 1234,
            binary_path: root.join("llama-server"),
            model_path: root.join("model.gguf"),
            model_sha256: "a".repeat(64),
            model_size_bytes: 1024,
            backend_release: LLAMA_CPP_RELEASE.release_tag.to_string(),
            binary_sha256: "b".repeat(64),
            mmproj: "not-required-text-only".to_string(),
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            ctx_size: Some(4096),
            stdout_log: root.join("stdout.log"),
            stderr_log: root.join("stderr.log"),
            started_at_ms: now_ms(),
        };
        backend_state::write_sidecar_record(&record).unwrap();
        let expected = format!(
            "backend_id={}\npid={}\nbinary_path={}\nmodel_path={}\nmodel_sha256={}\nmodel_size_bytes={}\nbackend_release={}\nbinary_sha256={}\nmmproj={}\nhost={}\nport={}\nctx_size={}\nstdout_log={}\nstderr_log={}\nstarted_at_ms={}\n",
            record.backend_id,
            record.pid,
            record.binary_path.display(),
            record.model_path.display(),
            record.model_sha256,
            record.model_size_bytes,
            record.backend_release,
            record.binary_sha256,
            record.mmproj,
            record.host,
            record.port,
            record.ctx_size.unwrap(),
            record.stdout_log.display(),
            record.stderr_log.display(),
            record.started_at_ms
        );
        assert_eq!(
            fs::read_to_string(backend_state::sidecar_record_path()).unwrap(),
            expected
        );
        let restored = backend_state::read_sidecar_record().unwrap().unwrap();

        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        fs::remove_dir_all(root).unwrap();

        assert_eq!(restored.ctx_size, Some(4096));
    }

    #[test]
    fn generation_record_codec_preserves_exact_bytes_and_round_trips() {
        let record = BackendGenerationRecord {
            generation_id: "generation-codec".to_string(),
            client_pid: 101,
            sidecar_pid: 202,
            started_at_ms: 303,
            timeout_ms: 404,
            streaming_display: true,
        };

        let rendered = render_generation_record(&record);

        assert_eq!(
            rendered,
            "generation_id=generation-codec\nclient_pid=101\nsidecar_pid=202\nstarted_at_ms=303\ntimeout_ms=404\nstreaming_display=true\n"
        );
        assert_eq!(parse_generation_record(&rendered), Some(record));
    }

    #[test]
    fn generation_start_does_not_delete_foreign_cancel_marker() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-generation-marker-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        fs::create_dir_all(paths::state_dir()).unwrap();
        state::atomic_replace_bytes(
            &backend_state::generation_cancel_path(),
            b"generation_id=another-generation\n",
        )
        .unwrap();
        let sidecar = BackendSidecarRecord {
            backend_id: LLAMA_CPP_BACKEND_ID.to_string(),
            pid: std::process::id(),
            binary_path: PathBuf::from("llama-server"),
            model_path: PathBuf::from("model.gguf"),
            model_sha256: "a".repeat(64),
            model_size_bytes: 1,
            backend_release: LLAMA_CPP_RELEASE.release_tag.to_string(),
            binary_sha256: "b".repeat(64),
            mmproj: "not-required-text-only".to_string(),
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            ctx_size: Some(4096),
            stdout_log: PathBuf::from("stdout.log"),
            stderr_log: PathBuf::from("stderr.log"),
            started_at_ms: now_ms(),
        };

        let generation = begin_active_generation(&sidecar, 1_000, false).unwrap();
        let marker = fs::read_to_string(backend_state::generation_cancel_path()).unwrap();

        assert!(marker.contains("generation_id=another-generation"));
        release_generation_admission(&generation.generation_id).unwrap();
        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancel_reports_the_recorded_terminal_outcome_and_cleans_generation_state() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-generation-terminal-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let generation = begin_active_generation(&generation_test_sidecar(), 1_000, true).unwrap();
        let generation_id = generation.generation_id.clone();
        let acknowledger = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if generation_cancel_requested(&generation_id).unwrap() {
                    write_generation_terminal_record(&generation_id, "completed", "event-done")
                        .unwrap();
                    release_generation_admission(&generation_id).unwrap();
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            panic!("generation cancellation marker가 생성되지 않았습니다.");
        });

        let report = cancel_generation_report().unwrap();
        acknowledger.join().unwrap();

        assert!(report.contains("status: acknowledged"));
        assert!(report.contains("terminal outcome: completed"));
        assert!(report.contains("terminal lifecycle event: event-done"));
        assert!(!backend_state::generation_record_path().exists());
        assert!(!backend_state::generation_lock_path().exists());
        assert!(!backend_state::generation_cancel_path().exists());
        assert!(!backend_state::generation_terminal_path(&generation.generation_id).exists());
        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parallel_generation_cancel_reaches_secondary_and_keeps_state_until_last_release() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-generation-group-cancel-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let sidecar = generation_test_sidecar();
        let primary = begin_active_generation(&sidecar, 1_000, false).unwrap();
        let secondary = begin_active_generation(&sidecar, 1_000, false).unwrap();
        assert_eq!(
            backend_state::read_generation_record()
                .unwrap()
                .unwrap()
                .generation_id,
            primary.generation_id
        );
        write_generation_terminal_record(&primary.generation_id, "completed", "event-primary")
            .unwrap();
        release_generation_admission(&primary.generation_id).unwrap();
        assert!(backend_state::generation_record_path().exists());

        let primary_id = primary.generation_id.clone();
        let secondary_id = secondary.generation_id.clone();
        let secondary_acknowledger = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if generation_cancel_requested(&secondary_id).unwrap() {
                    write_generation_terminal_record(&secondary_id, "cancelled", "event-secondary")
                        .unwrap();
                    let both_terminal_while_active =
                        backend_state::generation_terminal_path(&primary_id).exists()
                            && backend_state::generation_terminal_path(&secondary_id).exists()
                            && backend_state::generation_record_path().exists();
                    release_generation_admission(&secondary_id).unwrap();
                    return both_terminal_while_active;
                }
                thread::sleep(Duration::from_millis(10));
            }
            panic!("secondary generation이 primary cancel marker를 관찰하지 못했습니다.");
        });

        let report = cancel_generation_report().unwrap();
        assert!(secondary_acknowledger.join().unwrap());

        assert!(report.contains("status: acknowledged"));
        assert!(!backend_state::generation_record_path().exists());
        assert!(!backend_state::generation_lock_path().exists());
        assert!(!backend_state::generation_cancel_path().exists());
        backend_state::remove_generation_terminal_record(&secondary.generation_id).unwrap();
        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generation_stop_waits_for_terminal_acknowledgement_before_returning() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-generation-stop-order-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let generation = BackendGenerationRecord {
            generation_id: "generation-stop-order".to_string(),
            client_pid: std::process::id(),
            sidecar_pid: std::process::id(),
            started_at_ms: now_ms(),
            timeout_ms: 1_000,
            streaming_display: true,
        };
        backend_state::acquire_generation_lock(&generation).unwrap();
        write_backend_generation_record(&generation).unwrap();
        let generation_id = generation.generation_id.clone();
        let acknowledger = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if generation_cancel_requested(&generation_id).unwrap() {
                    write_generation_terminal_record(
                        &generation_id,
                        "cancelled",
                        "event-stop-cancelled",
                    )
                    .unwrap();
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
            panic!("backend stop cancellation marker가 생성되지 않았습니다.");
        });

        let outcome = cancel_active_generation_before_stop(&generation_test_sidecar()).unwrap();
        acknowledger.join().unwrap();

        assert_eq!(outcome, "cancelled");
        assert!(!backend_state::generation_record_path().exists());
        assert!(!backend_state::generation_lock_path().exists());
        assert!(!backend_state::generation_cancel_path().exists());
        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_generation_start_publishes_exactly_one_owner() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-generation-race-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        let sidecar = Arc::new(generation_test_sidecar());
        let barrier = Arc::new(Barrier::new(3));
        let contenders = (0..2)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let sidecar = Arc::clone(&sidecar);
                thread::spawn(move || {
                    barrier.wait();
                    begin_active_generation(&sidecar, 1_000, false)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = contenders
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>();
        let admitted = results
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .collect::<Vec<_>>();

        assert_eq!(admitted.len(), 2);
        let active = backend_state::read_generation_record().unwrap().unwrap();
        let lock = backend_state::read_generation_lock_record()
            .unwrap()
            .unwrap();
        assert!(admitted
            .iter()
            .any(|generation| generation.generation_id == active.generation_id));
        assert_eq!(lock.generation_id, active.generation_id);
        release_generation_admission(&admitted[0].generation_id).unwrap();
        assert_eq!(
            backend_state::read_generation_record()
                .unwrap()
                .unwrap()
                .generation_id,
            active.generation_id
        );
        release_generation_admission(&admitted[1].generation_id).unwrap();
        assert!(!backend_state::generation_record_path().exists());
        assert!(!backend_state::generation_lock_path().exists());
        let next = begin_active_generation(&sidecar, 1_000, false).unwrap();
        release_generation_admission(&next.generation_id).unwrap();
        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn stop_removes_stale_sidecar_record() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-backend-lifecycle-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));

        let model_path = root.join("model.gguf");
        fs::write(&model_path, b"fake model").unwrap();
        let record = BackendSidecarRecord {
            backend_id: LLAMA_CPP_BACKEND_ID.to_string(),
            pid: u32::MAX,
            binary_path: fs::canonicalize("/bin/sleep").unwrap(),
            model_path: fs::canonicalize(&model_path).unwrap(),
            model_sha256: checksum::sha256_file(&model_path).unwrap(),
            model_size_bytes: 10,
            backend_release: LLAMA_CPP_RELEASE.release_tag.to_string(),
            binary_sha256: checksum::sha256_file(Path::new("/bin/sleep")).unwrap(),
            mmproj: "not-required-text-only".to_string(),
            host: DEFAULT_HOST.to_string(),
            port: 65534,
            ctx_size: Some(4096),
            stdout_log: root.join("stdout.log"),
            stderr_log: root.join("stderr.log"),
            started_at_ms: now_ms(),
        };
        backend_state::write_sidecar_record(&record).unwrap();

        let status = status_report().unwrap();
        let stop = stop_report().unwrap();
        let record_after_stop = backend_state::read_sidecar_record().unwrap();

        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        env::remove_var(ENV_BACKEND_PORT);
        let _ = fs::remove_dir_all(root);

        assert!(status.contains("status: stale"));
        assert!(stop.contains("status: stale-record-removed"));
        assert!(record_after_stop.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn start_timeout_removes_record_and_keeps_logs() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = env::temp_dir().join(format!(
            "rpotato-backend-timeout-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("project")).unwrap();
        env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
        env::set_var(ENV_BACKEND_PORT, "65534");

        let backend_script = root.join("fake-llama-server-timeout");
        fs::write(
            &backend_script,
            "#!/bin/sh\necho 'booting stdout'\necho 'booting stderr' >&2\nexec sleep 10\n",
        )
        .unwrap();
        llama_install::set_executable_bit(&backend_script).unwrap();
        env::set_var(ENV_BACKEND_PATH, &backend_script);

        let model_path = root.join("model.gguf");
        fs::write(&model_path, b"fake model").unwrap();
        let err = start_sidecar_with_timeout(
            model_path.to_str().unwrap(),
            Some(4096),
            Duration::from_millis(200),
        )
        .unwrap_err();
        let stdout_logs = fs::read_dir(paths::logs_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("stdout"))
            .count();
        let stderr_logs = fs::read_dir(paths::logs_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("stderr"))
            .count();
        let record = backend_state::read_sidecar_record().unwrap();

        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        env::remove_var(ENV_BACKEND_PATH);
        env::remove_var(ENV_BACKEND_PORT);
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 3);
        assert!(err.message.contains("backend start timeout"));
        assert!(record.is_none());
        assert!(stdout_logs > 0);
        assert!(stderr_logs > 0);
    }

    #[cfg(unix)]
    #[test]
    fn unix_pid_arg_rejects_wrapping_values() {
        assert_eq!(backend_process::unix_pid_arg(0), None);
        assert_eq!(backend_process::unix_pid_arg(u32::MAX), None);
        assert_eq!(
            backend_process::unix_pid_arg(i32::MAX as u32),
            Some((i32::MAX as u32).to_string())
        );
    }

    #[test]
    fn health_check_report_is_diagnostic_not_process_start() {
        let report = health_check_report();
        assert!(report.contains("backend health check"));
        assert!(report.contains("health URL"));
        assert!(report.contains("timeout ms"));
    }

    #[test]
    fn model_id_comes_from_model_file_stem() {
        let model_id = model_id_from_path(Path::new("/tmp/Qwen3.5-4B-Q4_K_M.gguf"));

        assert_eq!(model_id, "Qwen3.5-4B-Q4_K_M");
    }

    fn write_test_tar_gz(path: &Path, files: &[(&str, &[u8])]) -> std::io::Result<()> {
        let file = File::create(path)?;
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (file_path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_path(file_path)?;
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append(&header, *bytes)?;
        }
        let encoder = builder.into_inner()?;
        encoder.finish()?;
        Ok(())
    }
}
