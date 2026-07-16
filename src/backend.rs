use std::collections::BTreeSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::layout as paths;
use crate::adapters::llama_cpp::stream as backend_stream;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::inference::backend::BackendAdapter;
use crate::runtime_core::inference::backend::{
    BackendChatRun, BackendChatSampling, MAX_CHAT_TIMEOUT_MS,
};
use crate::runtime_core::inference::{
    resource,
    stream::{StreamOutcome, StreamTermination},
};
use crate::{korean_guard, ledger, model, observability, state};

const LLAMA_CPP_BACKEND_ID: &str = "llama.cpp";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 17842;
const HEALTH_TIMEOUT_MS: u64 = 500;
const ENV_BACKEND_PATH: &str = "RPOTATO_BACKEND_LLAMA_CPP_PATH";
const ENV_BACKEND_PORT: &str = "RPOTATO_BACKEND_PORT";
const ENV_BACKEND_START_TRACE: &str = "RPOTATO_TEST_BACKEND_START_TRACE";
const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;
const VERSION_TIMEOUT_MS: u64 = 5_000;
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
struct GenerationAdmissionState {
    active_generation_ids: BTreeSet<String>,
    primary_generation_id: Option<String>,
}

static GENERATION_ADMISSION_STATE: Mutex<GenerationAdmissionState> =
    Mutex::new(GenerationAdmissionState {
        active_generation_ids: BTreeSet::new(),
        primary_generation_id: None,
    });
static BACKEND_RESOURCE_SAMPLE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LlamaCppAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendDiscovery {
    adapter_id: &'static str,
    binary_name: &'static str,
    managed_path: PathBuf,
    selected_path: PathBuf,
    selected_source: &'static str,
    override_path: Option<PathBuf>,
    binary_exists: bool,
    binary_is_file: bool,
    binary_executable: bool,
    host: &'static str,
    port: u16,
    port_source: &'static str,
    health_url: String,
}

#[derive(Debug, Clone, Copy)]
struct BackendReleaseManifest {
    id: &'static str,
    upstream_source: &'static str,
    license: &'static str,
    license_source: &'static str,
    license_checked_at: &'static str,
    release_tag: &'static str,
    release_url: &'static str,
    release_api_source: &'static str,
    release_checked_at: &'static str,
    artifacts: &'static [BackendReleaseArtifact],
    install_blockers: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackendReleaseArtifact {
    os: &'static str,
    arch: &'static str,
    archive_name: &'static str,
    archive_url: &'static str,
    archive_sha256: &'static str,
    archive_size_bytes: u64,
    archive_kind: BackendArchiveKind,
    binary_relative_path: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackendArchiveDownloadStatus {
    Downloaded,
    CacheHit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendInstallResult {
    download_status: BackendArchiveDownloadStatus,
    archive_path: PathBuf,
    extracted_binary: PathBuf,
    managed_binary: PathBuf,
    binary_sha256: String,
    ledger_event: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendInstallRecord {
    release_tag: String,
    archive_sha256: String,
    binary_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendVersionProbe {
    status: &'static str,
    command: String,
    exit_code: Option<i32>,
    output: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendSidecarRecord {
    backend_id: String,
    pid: u32,
    binary_path: PathBuf,
    model_path: PathBuf,
    model_sha256: String,
    model_size_bytes: u64,
    backend_release: String,
    binary_sha256: String,
    mmproj: String,
    host: String,
    port: u16,
    ctx_size: Option<u32>,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
    started_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendGenerationRecord {
    generation_id: String,
    client_pid: u32,
    sidecar_pid: u32,
    started_at_ms: u128,
    timeout_ms: u32,
    streaming_display: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackendGenerationTerminalRecord {
    generation_id: String,
    outcome: String,
    lifecycle_event: String,
    recorded_at_ms: u128,
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

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
struct BackendChatCompletion {
    content: String,
    finish_reason: String,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
struct BackendResourceSampleReport {
    metric: observability::ResourceSampleMetric,
    ledger_event: String,
    pressure: resource::ResourcePressure,
}

impl BackendArchiveKind {
    fn as_str(self) -> &'static str {
        match self {
            BackendArchiveKind::TarGz => "tar.gz",
            BackendArchiveKind::Zip => "zip",
        }
    }
}

const LLAMA_CPP_RELEASE: BackendReleaseManifest = BackendReleaseManifest {
    id: LLAMA_CPP_BACKEND_ID,
    upstream_source: "https://github.com/ggml-org/llama.cpp",
    license: "MIT",
    license_source: "https://github.com/ggml-org/llama.cpp/blob/b9982/LICENSE",
    license_checked_at: "2026-07-13",
    release_tag: "b9982",
    release_url: "https://github.com/ggml-org/llama.cpp/releases/tag/b9982",
    release_api_source: "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest",
    release_checked_at: "2026-07-13",
    artifacts: &LLAMA_CPP_RELEASE_ARTIFACTS,
    install_blockers: &[],
};

const LLAMA_CPP_RELEASE_ARTIFACTS: [BackendReleaseArtifact; 6] = [
    BackendReleaseArtifact {
        os: "macos",
        arch: "aarch64",
        archive_name: "llama-b9982-bin-macos-arm64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-macos-arm64.tar.gz",
        archive_sha256: "9606e3a609bc9483730f50f17ce78c3d764df8eaec63fcbb47d2f8b235667c9c",
        archive_size_bytes: 10_746_432,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "macos",
        arch: "x86_64",
        archive_name: "llama-b9982-bin-macos-x64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-macos-x64.tar.gz",
        archive_sha256: "da109cc18574392ab88936de826ca00f8d196b9ef5a1c19da72fbfb06bea7cd0",
        archive_size_bytes: 11_022_427,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "linux",
        arch: "aarch64",
        archive_name: "llama-b9982-bin-ubuntu-arm64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-ubuntu-arm64.tar.gz",
        archive_sha256: "9468c0282c15e286216a63122e7471f7d14888d3858bdab61b72d14a2531cf60",
        archive_size_bytes: 12_782_598,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "linux",
        arch: "x86_64",
        archive_name: "llama-b9982-bin-ubuntu-x64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-ubuntu-x64.tar.gz",
        archive_sha256: "0c1f0445f6f86a0f049de3586b7eabdde7108d827d0a9b2c5c0dc2185506ffee",
        archive_size_bytes: 15_850_588,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "windows",
        arch: "aarch64",
        archive_name: "llama-b9982-bin-win-cpu-arm64.zip",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-win-cpu-arm64.zip",
        archive_sha256: "11ad20d8df121d5760900b4e2fa9943a065856075ef44df52ed7a8dc58b08b2f",
        archive_size_bytes: 12_151_247,
        archive_kind: BackendArchiveKind::Zip,
        binary_relative_path: "llama-server.exe",
    },
    BackendReleaseArtifact {
        os: "windows",
        arch: "x86_64",
        archive_name: "llama-b9982-bin-win-cpu-x64.zip",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9982/llama-b9982-bin-win-cpu-x64.zip",
        archive_sha256: "69337038e8e56feb3c04d99588fa19f9241b294bae6f6c2e665a301605726e2a",
        archive_size_bytes: 18_247_652,
        archive_kind: BackendArchiveKind::Zip,
        binary_relative_path: "llama-server.exe",
    },
];

impl BackendAdapter for LlamaCppAdapter {
    fn id(&self) -> &'static str {
        LLAMA_CPP_BACKEND_ID
    }

    fn binary_name(&self) -> &'static str {
        if cfg!(target_os = "windows") {
            "llama-server.exe"
        } else {
            "llama-server"
        }
    }

    fn managed_binary_path(&self) -> PathBuf {
        paths::managed_backend_path()
    }

    fn default_host(&self) -> &'static str {
        DEFAULT_HOST
    }

    fn default_port(&self) -> u16 {
        DEFAULT_PORT
    }
}

pub fn doctor_summary() -> String {
    let discovery = discover_llama_cpp();
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
    let discovery = discover_llama_cpp();
    let version_probe = probe_backend_version(&discovery);
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
    let discovery = discover_llama_cpp();
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

    let archive_path = backend_archive_path(artifact);
    let download_status = download_backend_archive(artifact, &archive_path)?;
    verify_backend_archive_file(artifact, &archive_path)?;

    let managed_binary = LlamaCppAdapter.managed_binary_path();
    let staging_dir = backend_staging_dir(&LLAMA_CPP_RELEASE, artifact);
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
        display_download_status(result.download_status),
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
    let Some(record) = read_backend_sidecar_record()? else {
        return Ok(format!(
            "backend status\n- status: stopped\n- sidecar record: {}\n- 다음 단계: rpotato backend start --model <path> [--ctx-size <tokens>]",
            backend_sidecar_record_path().display()
        ));
    };

    let running = process_is_running(record.pid);
    let health = if running {
        Some(probe_health(
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
        backend_sidecar_record_path().display()
    ))
}

pub fn stop_report() -> Result<String, AppError> {
    let Some(record) = read_backend_sidecar_record()? else {
        return Ok(format!(
            "backend stop\n- status: stopped\n- sidecar record: {}\n- 동작: 실행 중인 managed sidecar record가 없어 no-op입니다.",
            backend_sidecar_record_path().display()
        ));
    };

    if !process_is_running(record.pid) {
        remove_file_if_exists(&backend_sidecar_record_path())?;
        let event_id = state::record_event(
            "backend.sidecar.stop.stale",
            "stale backend sidecar record 제거",
            &format!("pid={} binary={}", record.pid, record.binary_path.display()),
        )?;
        return Ok(format!(
            "backend stop\n- status: stale-record-removed\n- pid: {}\n- sidecar record: {}\n- ledger event: {}",
            record.pid,
            backend_sidecar_record_path().display(),
            event_id
        ));
    }

    let command_matched = process_command_matches_record(&record);

    let generation_outcome = cancel_active_generation_before_stop(&record)?;

    terminate_process_with_fallback(record.pid)?;
    remove_file_if_exists(&backend_sidecar_record_path())?;
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
        || terminate_process(pid, false),
        || terminate_process(pid, true),
        || process_running_status(pid),
        || wait_until_process_stops_checked(pid, Duration::from_millis(STOP_TIMEOUT_MS)),
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
    if let Some(generation) = read_backend_generation_record()? {
        if generation.sidecar_pid == record.pid && process_is_running(generation.client_pid) {
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
                remove_generation_state_if_owned_checked(&generation.generation_id)?;
                remove_file_if_exists(&backend_generation_terminal_path(
                    &generation.generation_id,
                ))?;
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
            remove_generation_state_if_owned_checked(&generation.generation_id)?;
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
    let discovery = discover_llama_cpp();
    let probe = probe_health(
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
    let Some(record) = read_backend_sidecar_record()? else {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: 실행 중인 sidecar record가 없습니다.\n- 다음 단계: rpotato backend start --model <path> --ctx-size 4096\n- sidecar record: {}",
            backend_sidecar_record_path().display()
        )));
    };
    if !process_is_running(record.pid) {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: sidecar record는 있지만 process가 실행 중이 아닙니다.\n- pid: {}\n- 다음 단계: rpotato backend stop으로 stale record를 정리한 뒤 다시 시작하세요.",
            record.pid
        )));
    }

    let health = probe_health(
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
    let body = chat_request_body(
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
                quantization: model::quantization_for_artifact_hash(&record.model_sha256)
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
        quantization: model::quantization_for_artifact_hash(&record.model_sha256)
            .map(str::to_string),
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
        quantization: model::quantization_for_artifact_hash(&record.model_sha256)
            .map(str::to_string),
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
    let Some(record) = read_backend_generation_record()? else {
        return Ok(format!(
            "backend generation 취소\n- status: idle\n- active generation record: {}",
            backend_generation_record_path().display()
        ));
    };
    if !process_is_running(record.client_pid) {
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
        remove_file_if_exists(&backend_generation_terminal_path(&record.generation_id))?;
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

fn discover_llama_cpp() -> BackendDiscovery {
    let adapter = LlamaCppAdapter;
    let managed_path = adapter.managed_binary_path();
    let override_path = env::var_os(ENV_BACKEND_PATH).map(PathBuf::from);
    let (selected_path, selected_source) = match &override_path {
        Some(path) => (path.clone(), "env override"),
        None => (managed_path.clone(), "managed"),
    };
    let (port, port_source) = configured_port(adapter.default_port());
    let health_url = format!("http://{}:{}/health", adapter.default_host(), port);

    BackendDiscovery {
        adapter_id: adapter.id(),
        binary_name: adapter.binary_name(),
        managed_path,
        selected_path: selected_path.clone(),
        selected_source,
        override_path,
        binary_exists: selected_path.exists(),
        binary_is_file: selected_path.is_file(),
        binary_executable: is_executable(&selected_path),
        host: adapter.default_host(),
        port,
        port_source,
        health_url,
    }
}

struct HealthProbe {
    status: &'static str,
    tcp_connected: bool,
    http_status_line: Option<String>,
    error: Option<String>,
}

fn probe_health(host: &str, port: u16, timeout: Duration) -> HealthProbe {
    let address = format!("{host}:{port}");
    let Ok(mut addresses) = address.to_socket_addrs() else {
        return HealthProbe {
            status: "unreachable",
            tcp_connected: false,
            http_status_line: None,
            error: Some(format!("address resolve 실패: {address}")),
        };
    };
    let Some(socket_addr) = addresses.next() else {
        return HealthProbe {
            status: "unreachable",
            tcp_connected: false,
            http_status_line: None,
            error: Some(format!("address 없음: {address}")),
        };
    };

    let Ok(mut stream) = TcpStream::connect_timeout(&socket_addr, timeout) else {
        return HealthProbe {
            status: "unreachable",
            tcp_connected: false,
            http_status_line: None,
            error: Some(format!("connect 실패: {socket_addr}")),
        };
    };

    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let request =
        format!("GET /health HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    if let Err(err) = stream.write_all(request.as_bytes()) {
        return HealthProbe {
            status: "unhealthy",
            tcp_connected: true,
            http_status_line: None,
            error: Some(format!("health request write 실패: {err}")),
        };
    }

    let mut response = Vec::with_capacity(256);
    let status_line = loop {
        if let Some(status_line) = first_http_status_line(&response) {
            break status_line;
        }
        if response.len() >= 8 * 1024 {
            return HealthProbe {
                status: "unhealthy",
                tcp_connected: true,
                http_status_line: None,
                error: Some("health response status line이 8 KiB를 초과했습니다.".to_string()),
            };
        }
        let mut buffer = [0_u8; 256];
        match stream.read(&mut buffer) {
            Ok(0) => {
                return HealthProbe {
                    status: "unhealthy",
                    tcp_connected: true,
                    http_status_line: None,
                    error: Some("health response가 status line 전에 종료됐습니다.".to_string()),
                };
            }
            Ok(read) => response.extend_from_slice(&buffer[..read]),
            Err(err) => {
                return HealthProbe {
                    status: "unhealthy",
                    tcp_connected: true,
                    http_status_line: None,
                    error: Some(format!("health response read 실패: {err}")),
                };
            }
        }
    };
    let status = if status_line.contains(" 200 ") || status_line.ends_with(" 200") {
        "healthy"
    } else {
        "unhealthy"
    };

    HealthProbe {
        status,
        tcp_connected: true,
        http_status_line: Some(if status_line.is_empty() {
            "없음".to_string()
        } else {
            status_line
        }),
        error: None,
    }
}

fn first_http_status_line(response: &[u8]) -> Option<String> {
    let end = response.iter().position(|byte| *byte == b'\n')?;
    let line = response[..end]
        .strip_suffix(b"\r")
        .unwrap_or(&response[..end]);
    std::str::from_utf8(line).ok().map(str::to_string)
}

fn chat_request_body(
    model_path: &Path,
    prompt: &str,
    max_tokens: u32,
    sampling: &BackendChatSampling,
    stream: bool,
) -> String {
    let system_prompt = "사용자에게 보이는 최종 답변만 한국어로 작성합니다. reasoning trace, <think> 태그, 내부 추론은 출력하지 않습니다.";
    let template_options = if model_id_from_path(model_path)
        .to_ascii_lowercase()
        .starts_with("qwen")
    {
        ",\"chat_template_kwargs\":{\"enable_thinking\":false}"
    } else {
        ""
    };
    let stream_options = if stream {
        ",\"stream\":true,\"stream_options\":{\"include_usage\":true}"
    } else {
        ""
    };
    format!(
        "{{\"messages\":[{{\"role\":\"system\",\"content\":\"{}\"}},{{\"role\":\"user\",\"content\":\"{}\"}}],\"max_tokens\":{},\"temperature\":{},\"top_p\":{}{}{}}}",
        ledger::json_string(system_prompt),
        ledger::json_string(prompt),
        max_tokens,
        sampling.temperature,
        sampling.top_p,
        template_options,
        stream_options
    )
}

#[cfg(test)]
fn parse_chat_completion_response(body: &str) -> Option<BackendChatCompletion> {
    Some(BackendChatCompletion {
        content: extract_json_string_value(body, "content")?,
        finish_reason: extract_json_string_value(body, "finish_reason")
            .unwrap_or_else(|| "unknown".to_string()),
        prompt_tokens: extract_json_u32_value(body, "prompt_tokens"),
        completion_tokens: extract_json_u32_value(body, "completion_tokens"),
        total_tokens: extract_json_u32_value(body, "total_tokens"),
    })
}

#[cfg(test)]
fn extract_json_string_value(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    let mut chars = text[start..].chars().peekable();
    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }
    if chars.next()? != '"' {
        return None;
    }

    let mut value = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                '/' => value.push('/'),
                'b' => value.push('\u{0008}'),
                'f' => value.push('\u{000c}'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                'u' => {
                    let mut code = String::new();
                    for _ in 0..4 {
                        code.push(chars.next()?);
                    }
                    let scalar = u32::from_str_radix(&code, 16).ok()?;
                    value.push(char::from_u32(scalar)?);
                }
                other => value.push(other),
            },
            other => value.push(other),
        }
    }
    None
}

#[cfg(test)]
fn extract_json_u32_value(text: &str, key: &str) -> Option<u32> {
    let needle = format!("\"{key}\":");
    let start = text.find(&needle)? + needle.len();
    let trimmed = text[start..].trim_start();
    let number: String = trimmed
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if number.is_empty() {
        return None;
    }
    number.parse::<u32>().ok()
}

#[cfg(test)]
fn strip_reasoning_trace(content: &str) -> (String, bool) {
    let mut output = String::new();
    let mut rest = content;
    let mut stripped = false;

    while let Some(start) = rest.find("<think>") {
        stripped = true;
        output.push_str(&rest[..start]);
        let after_start = &rest[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            return (output, true);
        };
        rest = &after_start[end + "</think>".len()..];
    }
    output.push_str(rest);
    (output, stripped)
}

fn backend_archive_path(artifact: &BackendReleaseArtifact) -> PathBuf {
    paths::downloads_dir().join(artifact.archive_name)
}

fn backend_staging_dir(
    manifest: &BackendReleaseManifest,
    artifact: &BackendReleaseArtifact,
) -> PathBuf {
    paths::backends_dir().join("llama.cpp").join(format!(
        ".staging-{}-{}-{}",
        manifest.release_tag, artifact.os, artifact.arch
    ))
}

fn download_backend_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
) -> Result<BackendArchiveDownloadStatus, AppError> {
    if archive_path.exists() && !archive_path.is_file() {
        return Err(AppError::blocked(format!(
            "backend archive cache path가 file이 아닙니다: {}",
            archive_path.display()
        )));
    }
    if archive_path.is_file() && backend_archive_matches(artifact, archive_path) {
        return Ok(BackendArchiveDownloadStatus::CacheHit);
    }

    let parent = archive_path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend archive parent path를 계산하지 못했습니다: {}",
            archive_path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend archive download directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    let part_path = archive_path.with_file_name(format!("{}.part", artifact.archive_name));
    remove_file_if_exists(&part_path)?;

    let response = ureq::get(artifact.archive_url)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|err| {
            AppError::runtime(format!(
                "backend archive 다운로드 실패\n- url: {}\n- error: {err}",
                artifact.archive_url
            ))
        })?;
    let (_, body) = response.into_parts();
    let mut reader = body.into_reader();
    let mut file = File::create(&part_path).map_err(|err| {
        AppError::runtime(format!(
            "backend archive partial file을 만들지 못했습니다: {} ({err})",
            part_path.display()
        ))
    })?;

    let copied_bytes =
        match copy_reader_with_limit(&mut reader, &mut file, artifact.archive_size_bytes) {
            Ok(copied_bytes) => copied_bytes,
            Err(err) => {
                drop(file);
                let _ = fs::remove_file(&part_path);
                return Err(err);
            }
        };
    file.sync_all().map_err(|err| {
        AppError::runtime(format!(
            "backend archive partial file sync 실패: {} ({err})",
            part_path.display()
        ))
    })?;
    drop(file);

    if copied_bytes != artifact.archive_size_bytes {
        remove_file_if_exists(&part_path)?;
        return Err(AppError::blocked(format!(
            "backend archive size 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_size_bytes,
            copied_bytes,
            part_path.display()
        )));
    }
    if let Err(err) = verify_backend_archive_file(artifact, &part_path) {
        remove_file_if_exists(&part_path)?;
        return Err(err);
    }

    remove_file_if_exists(archive_path)?;
    fs::rename(&part_path, archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend archive cache 배치 실패: {} -> {} ({err})",
            part_path.display(),
            archive_path.display()
        ))
    })?;

    Ok(BackendArchiveDownloadStatus::Downloaded)
}

fn backend_archive_matches(artifact: &BackendReleaseArtifact, archive_path: &Path) -> bool {
    verify_backend_archive_file(artifact, archive_path).is_ok()
}

fn verify_backend_archive_file(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
) -> Result<(), AppError> {
    let metadata = archive_path.metadata().map_err(|err| {
        AppError::runtime(format!(
            "backend archive metadata를 읽지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::blocked(format!(
            "backend archive path가 file이 아닙니다: {}",
            archive_path.display()
        )));
    }
    if metadata.len() != artifact.archive_size_bytes {
        return Err(AppError::blocked(format!(
            "backend archive size 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_size_bytes,
            metadata.len(),
            archive_path.display()
        )));
    }

    let actual_sha256 = checksum::sha256_file(archive_path)?;
    if !actual_sha256.eq_ignore_ascii_case(artifact.archive_sha256) {
        return Err(AppError::blocked(format!(
            "backend archive SHA-256 검증 실패\n- expected: {}\n- actual: {}\n- path: {}",
            artifact.archive_sha256,
            actual_sha256,
            archive_path.display()
        )));
    }

    Ok(())
}

fn copy_reader_with_limit<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    expected_bytes: u64,
) -> Result<u64, AppError> {
    let mut copied_bytes = 0_u64;
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_BYTES];

    loop {
        let bytes_read = reader.read(&mut buffer).map_err(|err| {
            AppError::runtime(format!("backend archive download stream read 실패: {err}"))
        })?;
        if bytes_read == 0 {
            break;
        }
        copied_bytes += bytes_read as u64;
        if copied_bytes > expected_bytes {
            return Err(AppError::blocked(format!(
                "backend archive size limit 초과\n- expected: {}\n- actual-at-least: {}",
                expected_bytes, copied_bytes
            )));
        }
        writer.write_all(&buffer[..bytes_read]).map_err(|err| {
            AppError::runtime(format!("backend archive partial file write 실패: {err}"))
        })?;
    }

    writer
        .flush()
        .map_err(|err| AppError::runtime(format!("backend archive flush 실패: {err}")))?;
    Ok(copied_bytes)
}

fn install_backend_from_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
    managed_binary: &Path,
    staging_dir: &Path,
    download_status: BackendArchiveDownloadStatus,
) -> Result<BackendInstallResult, AppError> {
    remove_dir_if_exists(staging_dir)?;
    fs::create_dir_all(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend staging directory를 만들지 못했습니다: {} ({err})",
            staging_dir.display()
        ))
    })?;

    if let Err(err) = extract_backend_archive(artifact, archive_path, staging_dir) {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(err);
    }
    let extracted_binary = match find_extracted_binary(artifact, staging_dir) {
        Ok(path) => path,
        Err(err) => {
            let _ = fs::remove_dir_all(staging_dir);
            return Err(err);
        }
    };
    if let Err(err) = place_managed_backend_payload(&extracted_binary, staging_dir, managed_binary)
    {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(err);
    }
    let binary_sha256 = checksum::sha256_file(managed_binary)?;
    write_backend_install_record(artifact, &binary_sha256)?;
    remove_dir_if_exists(staging_dir)?;

    let event_id = state::record_event(
        "backend.install.completed",
        "llama.cpp backend 설치 완료",
        &format!(
            "release_tag={} archive={} sha256={} managed_binary={} binary_sha256={} download_status={}",
            LLAMA_CPP_RELEASE.release_tag,
            archive_path.display(),
            artifact.archive_sha256,
            managed_binary.display(),
            binary_sha256,
            display_download_status(download_status)
        ),
    )?;

    Ok(BackendInstallResult {
        download_status,
        archive_path: archive_path.to_path_buf(),
        extracted_binary,
        managed_binary: managed_binary.to_path_buf(),
        binary_sha256,
        ledger_event: event_id,
    })
}

fn extract_backend_archive(
    artifact: &BackendReleaseArtifact,
    archive_path: &Path,
    staging_dir: &Path,
) -> Result<(), AppError> {
    match artifact.archive_kind {
        BackendArchiveKind::TarGz => extract_tar_gz_archive(archive_path, staging_dir),
        BackendArchiveKind::Zip => extract_zip_archive(archive_path, staging_dir),
    }
}

fn extract_tar_gz_archive(archive_path: &Path, staging_dir: &Path) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend tar.gz archive를 열지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend tar.gz archive extraction 실패: {} -> {} ({err})",
            archive_path.display(),
            staging_dir.display()
        ))
    })
}

fn extract_zip_archive(archive_path: &Path, staging_dir: &Path) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive를 열지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive metadata를 읽지 못했습니다: {} ({err})",
            archive_path.display()
        ))
    })?;
    archive.extract(staging_dir).map_err(|err| {
        AppError::runtime(format!(
            "backend zip archive extraction 실패: {} -> {} ({err})",
            archive_path.display(),
            staging_dir.display()
        ))
    })
}

fn find_extracted_binary(
    artifact: &BackendReleaseArtifact,
    staging_dir: &Path,
) -> Result<PathBuf, AppError> {
    let hinted_path = staging_dir.join(artifact.binary_relative_path);
    if is_regular_file_no_symlink(&hinted_path) {
        return Ok(hinted_path);
    }

    let binary_name = Path::new(artifact.binary_relative_path)
        .file_name()
        .ok_or_else(|| {
            AppError::blocked(format!(
                "archive 내부 binary path가 유효하지 않습니다: {}",
                artifact.binary_relative_path
            ))
        })?;
    let mut matches = Vec::new();
    collect_binary_matches(staging_dir, binary_name, &mut matches)?;
    matches.sort();

    match matches.len() {
        0 => Err(AppError::blocked(format!(
            "backend archive에서 binary를 찾지 못했습니다\n- expected: {}\n- staging: {}",
            artifact.binary_relative_path,
            staging_dir.display()
        ))),
        1 => Ok(matches.remove(0)),
        _ => Err(AppError::blocked(format!(
            "backend archive에서 binary 후보가 여러 개입니다\n- expected: {}\n- count: {}\n- staging: {}",
            artifact.binary_relative_path,
            matches.len(),
            staging_dir.display()
        ))),
    }
}

fn collect_binary_matches(
    directory: &Path,
    binary_name: &std::ffi::OsStr,
    matches: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(directory).map_err(|err| {
        AppError::runtime(format!(
            "backend extraction directory를 읽지 못했습니다: {} ({err})",
            directory.display()
        ))
    })? {
        let entry =
            entry.map_err(|err| AppError::runtime(format!("directory entry read 실패: {err}")))?;
        let path = entry.path();
        let file_type = fs::symlink_metadata(&path)
            .map_err(|err| {
                AppError::runtime(format!(
                    "backend extracted path metadata를 읽지 못했습니다: {} ({err})",
                    path.display()
                ))
            })?
            .file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_binary_matches(&path, binary_name, matches)?;
        } else if file_type.is_file() && path.file_name() == Some(binary_name) {
            matches.push(path);
        }
    }
    Ok(())
}

fn is_regular_file_no_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn place_managed_backend_payload(
    extracted_binary: &Path,
    staging_dir: &Path,
    managed_binary: &Path,
) -> Result<(), AppError> {
    let parent = managed_binary.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "managed backend binary parent path를 계산하지 못했습니다: {}",
            managed_binary.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "managed backend directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    if managed_binary.exists() && !managed_binary.is_file() {
        return Err(AppError::blocked(format!(
            "managed backend path가 file이 아닙니다: {}",
            managed_binary.display()
        )));
    }
    if parent.exists() && !parent.is_dir() {
        return Err(AppError::blocked(format!(
            "managed backend directory path가 directory가 아닙니다: {}",
            parent.display()
        )));
    }

    let file_name = managed_binary
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("llama-server");
    let next_dir = parent.with_file_name("llama.cpp.next");
    let backup_dir = parent.with_file_name("llama.cpp.previous");
    remove_dir_if_exists(&next_dir)?;
    remove_dir_if_exists(&backup_dir)?;

    let payload_root = payload_root_for(extracted_binary, staging_dir)?;
    copy_release_tree(&payload_root, &next_dir)?;
    let next_binary = next_dir.join(file_name);
    if !next_binary.is_file() {
        fs::copy(extracted_binary, &next_binary).map_err(|err| {
            AppError::runtime(format!(
                "managed backend binary copy 실패: {} -> {} ({err})",
                extracted_binary.display(),
                next_binary.display()
            ))
        })?;
    }
    set_executable_bit(&next_binary)?;

    let had_existing = parent.exists();
    if had_existing {
        fs::rename(parent, &backup_dir).map_err(|err| {
            AppError::runtime(format!(
                "기존 managed backend directory backup 실패: {} -> {} ({err})",
                parent.display(),
                backup_dir.display()
            ))
        })?;
    }

    if let Err(err) = fs::rename(&next_dir, parent) {
        if had_existing && backup_dir.is_dir() {
            let _ = fs::rename(&backup_dir, parent);
        }
        let _ = fs::remove_dir_all(&next_dir);
        return Err(AppError::runtime(format!(
            "managed backend directory 배치 실패: {} -> {} ({err})",
            next_dir.display(),
            parent.display()
        )));
    }
    remove_dir_if_exists(&backup_dir)?;

    Ok(())
}

fn payload_root_for(extracted_binary: &Path, staging_dir: &Path) -> Result<PathBuf, AppError> {
    if !extracted_binary.starts_with(staging_dir) {
        return Err(AppError::runtime(format!(
            "extracted backend binary relative path 계산 실패: {} under {}",
            extracted_binary.display(),
            staging_dir.display()
        )));
    }
    let parent = extracted_binary.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "extracted backend binary parent path를 계산하지 못했습니다: {}",
            extracted_binary.display()
        ))
    })?;
    Ok(parent.to_path_buf())
}

fn copy_release_tree(source: &Path, destination: &Path) -> Result<(), AppError> {
    fs::create_dir_all(destination).map_err(|err| {
        AppError::runtime(format!(
            "managed backend payload directory를 만들지 못했습니다: {} ({err})",
            destination.display()
        ))
    })?;
    for entry in fs::read_dir(source).map_err(|err| {
        AppError::runtime(format!(
            "backend payload source directory를 읽지 못했습니다: {} ({err})",
            source.display()
        ))
    })? {
        let entry = entry
            .map_err(|err| AppError::runtime(format!("backend payload entry read 실패: {err}")))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = fs::symlink_metadata(&source_path)
            .map_err(|err| {
                AppError::runtime(format!(
                    "backend payload metadata를 읽지 못했습니다: {} ({err})",
                    source_path.display()
                ))
            })?
            .file_type();

        if file_type.is_dir() {
            copy_release_tree(&source_path, &destination_path)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            fs::copy(&source_path, &destination_path).map_err(|err| {
                AppError::runtime(format!(
                    "backend payload file copy 실패: {} -> {} ({err})",
                    source_path.display(),
                    destination_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable_bit(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "managed backend binary metadata를 읽지 못했습니다: {} ({err})",
                path.display()
            ))
        })?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    fs::set_permissions(path, permissions).map_err(|err| {
        AppError::runtime(format!(
            "managed backend binary 실행 권한 설정 실패: {} ({err})",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn set_executable_bit(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_file(path).map_err(|err| {
            AppError::runtime(format!("file 삭제 실패: {} ({err})", path.display()))
        })?;
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|err| {
            AppError::runtime(format!("directory 삭제 실패: {} ({err})", path.display()))
        })?;
    }
    Ok(())
}

fn display_download_status(status: BackendArchiveDownloadStatus) -> &'static str {
    match status {
        BackendArchiveDownloadStatus::Downloaded => "downloaded",
        BackendArchiveDownloadStatus::CacheHit => "cache-hit",
    }
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
    prune_generation_terminal_records();
    let mut publish_primary = true;
    if let Some(active) = read_backend_generation_record()? {
        if process_is_running(active.client_pid) {
            if active.client_pid == std::process::id()
                && active.sidecar_pid == sidecar.pid
                && !admission.active_generation_ids.is_empty()
                && admission.primary_generation_id.as_deref() == Some(active.generation_id.as_str())
            {
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
    } else if let Some(lock) = read_backend_generation_lock_record()? {
        if process_is_running(lock.client_pid) {
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
        acquire_backend_generation_lock(&record)?;
        if let Err(err) = write_backend_generation_record(&record) {
            let _ = remove_generation_lock_if_owned_checked(&record.generation_id);
            return Err(err);
        }
        admission.primary_generation_id = Some(record.generation_id.clone());
    }
    if !admission
        .active_generation_ids
        .insert(record.generation_id.clone())
    {
        if publish_primary {
            admission.primary_generation_id = None;
            remove_generation_state_if_owned_checked(&record.generation_id)?;
        }
        return Err(AppError::blocked(
            "backend generation admission id collision",
        ));
    }
    Ok(record)
}

fn acquire_backend_generation_lock(record: &BackendGenerationRecord) -> Result<(), AppError> {
    let path = backend_generation_lock_path();
    let parent = path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend generation lock parent path를 계산하지 못했습니다: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend generation lock directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|err| {
            AppError::blocked(format!(
                "backend generation lease를 획득하지 못했습니다: {} ({err})",
                path.display()
            ))
        })?;
    if let Err(err) = file
        .write_all(render_backend_generation_record(record).as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(AppError::runtime(format!(
            "backend generation lease를 기록하지 못했습니다: {} ({err})",
            path.display()
        )));
    }
    Ok(())
}

fn write_backend_generation_record(record: &BackendGenerationRecord) -> Result<(), AppError> {
    let path = backend_generation_record_path();
    let contents = render_backend_generation_record(record);
    state::atomic_replace_bytes(&path, contents.as_bytes())
}

fn render_backend_generation_record(record: &BackendGenerationRecord) -> String {
    format!(
        "generation_id={}\nclient_pid={}\nsidecar_pid={}\nstarted_at_ms={}\ntimeout_ms={}\nstreaming_display={}\n",
        record.generation_id,
        record.client_pid,
        record.sidecar_pid,
        record.started_at_ms,
        record.timeout_ms,
        record.streaming_display
    )
}

fn read_backend_generation_record() -> Result<Option<BackendGenerationRecord>, AppError> {
    let path = backend_generation_record_path();
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend generation record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_backend_generation_record(&contents)
        .map(Some)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "backend generation record 형식이 유효하지 않습니다: {}",
                path.display()
            ))
        })
}

fn read_backend_generation_lock_record() -> Result<Option<BackendGenerationRecord>, AppError> {
    let path = backend_generation_lock_path();
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend generation lock을 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_backend_generation_record(&contents)
        .map(Some)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "backend generation lock 형식이 유효하지 않습니다: {}",
                path.display()
            ))
        })
}

fn parse_backend_generation_record(contents: &str) -> Option<BackendGenerationRecord> {
    Some(BackendGenerationRecord {
        generation_id: record_value(contents, "generation_id")?.to_string(),
        client_pid: record_value(contents, "client_pid")?.parse().ok()?,
        sidecar_pid: record_value(contents, "sidecar_pid")?.parse().ok()?,
        started_at_ms: record_value(contents, "started_at_ms")?.parse().ok()?,
        timeout_ms: record_value(contents, "timeout_ms")?.parse().ok()?,
        streaming_display: record_value(contents, "streaming_display")?.parse().ok()?,
    })
}

fn record_value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    contents.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn generation_cancel_requested(generation_id: &str) -> Result<bool, AppError> {
    let path = backend_generation_cancel_path();
    if !path.exists() {
        return Ok(false);
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend generation cancel marker를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let Some(cancel_generation_id) = record_value(&contents, "generation_id") else {
        return Ok(false);
    };
    if cancel_generation_id == generation_id {
        return Ok(true);
    }
    let admission = GENERATION_ADMISSION_STATE
        .lock()
        .map_err(|_| AppError::runtime("backend generation admission lock poisoned"))?;
    Ok(
        admission.primary_generation_id.as_deref() == Some(cancel_generation_id)
            && admission.active_generation_ids.contains(generation_id),
    )
}

fn write_generation_cancel_marker(generation_id: &str) -> Result<(), AppError> {
    let marker = format!(
        "generation_id={}\nrequested_at_ms={}\nrequester_pid={}\n",
        generation_id,
        now_ms(),
        std::process::id()
    );
    state::atomic_replace_bytes(&backend_generation_cancel_path(), marker.as_bytes())
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
        &backend_generation_terminal_path(generation_id),
        contents.as_bytes(),
    )
}

fn read_generation_terminal_record(
    generation_id: &str,
) -> Result<Option<BackendGenerationTerminalRecord>, AppError> {
    let path = backend_generation_terminal_path(generation_id);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend generation terminal record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let record = BackendGenerationTerminalRecord {
        generation_id: record_value(&contents, "generation_id")
            .ok_or_else(|| AppError::blocked("generation terminal id가 없습니다."))?
            .to_string(),
        outcome: record_value(&contents, "outcome")
            .ok_or_else(|| AppError::blocked("generation terminal outcome이 없습니다."))?
            .to_string(),
        lifecycle_event: record_value(&contents, "lifecycle_event")
            .ok_or_else(|| AppError::blocked("generation terminal lifecycle event가 없습니다."))?
            .to_string(),
        recorded_at_ms: record_value(&contents, "recorded_at_ms")
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| {
                AppError::blocked("generation terminal timestamp가 유효하지 않습니다.")
            })?,
    };
    if record.generation_id != generation_id {
        return Err(AppError::blocked(
            "generation terminal record id가 요청과 일치하지 않습니다.",
        ));
    }
    Ok(Some(record))
}

fn wait_for_generation_terminal(
    generation_id: &str,
    timeout: Duration,
) -> Result<Option<BackendGenerationTerminalRecord>, AppError> {
    let started = Instant::now();
    loop {
        if let Some(record) = read_generation_terminal_record(generation_id)? {
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
        let released = read_backend_generation_record()?
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

fn prune_generation_terminal_records() {
    let directory = paths::state_dir().join("backend-generation-terminals");
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let now = now_ms();
    for entry in entries.flatten() {
        let path = entry.path();
        let old = fs::read_to_string(&path)
            .ok()
            .and_then(|contents| {
                record_value(&contents, "recorded_at_ms")?
                    .parse::<u128>()
                    .ok()
            })
            .map(|recorded| now.saturating_sub(recorded) > TERMINAL_RECORD_RETENTION_MS)
            .unwrap_or(false);
        if old {
            let _ = fs::remove_file(path);
        }
    }
}

fn remove_generation_state_if_owned(generation_id: &str) {
    let _ = remove_generation_state_if_owned_checked(generation_id);
}

fn release_generation_admission(generation_id: &str) -> Result<(), AppError> {
    let mut admission = GENERATION_ADMISSION_STATE
        .lock()
        .map_err(|_| AppError::runtime("backend generation admission lock poisoned"))?;
    if admission.active_generation_ids.is_empty() {
        return remove_generation_state_if_owned_checked(generation_id);
    }
    if !admission.active_generation_ids.remove(generation_id) {
        return Err(AppError::blocked(format!(
            "backend generation admission release binding 누락: {generation_id}"
        )));
    }
    if !admission.active_generation_ids.is_empty() {
        return Ok(());
    }
    let primary_generation_id = admission
        .primary_generation_id
        .take()
        .unwrap_or_else(|| generation_id.to_string());
    remove_generation_state_if_owned_checked(&primary_generation_id)
}

fn remove_generation_state_if_owned_checked(generation_id: &str) -> Result<(), AppError> {
    let record_path = backend_generation_record_path();
    let owned = fs::read_to_string(&record_path)
        .ok()
        .and_then(|contents| {
            record_value(&contents, "generation_id").map(|value| value == generation_id)
        })
        .unwrap_or(false);
    if owned {
        remove_file_if_exists(&record_path)?;
    }
    let cancel_path = backend_generation_cancel_path();
    let owned_marker = fs::read_to_string(&cancel_path)
        .ok()
        .and_then(|contents| {
            record_value(&contents, "generation_id").map(|value| value == generation_id)
        })
        .unwrap_or(false);
    if owned_marker {
        remove_file_if_exists(&cancel_path)?;
    }
    remove_generation_lock_if_owned_checked(generation_id)?;
    Ok(())
}

fn remove_generation_lock_if_owned_checked(generation_id: &str) -> Result<(), AppError> {
    let path = backend_generation_lock_path();
    let owned = fs::read_to_string(&path)
        .ok()
        .and_then(|contents| {
            record_value(&contents, "generation_id").map(|value| value == generation_id)
        })
        .unwrap_or(false);
    if owned {
        remove_file_if_exists(&path)?;
    }
    Ok(())
}

fn backend_install_record_path() -> PathBuf {
    paths::backends_dir()
        .join("llama.cpp")
        .join("install-record.txt")
}

fn backend_sidecar_record_path() -> PathBuf {
    paths::state_dir().join("backend-llama.cpp-sidecar.txt")
}

fn backend_generation_record_path() -> PathBuf {
    paths::state_dir().join("backend-active-generation.txt")
}

fn backend_generation_lock_path() -> PathBuf {
    paths::state_dir().join("backend-active-generation.lock")
}

fn backend_generation_cancel_path() -> PathBuf {
    paths::state_dir().join("backend-active-generation.cancel")
}

fn backend_generation_terminal_path(generation_id: &str) -> PathBuf {
    paths::state_dir()
        .join("backend-generation-terminals")
        .join(format!("{generation_id}.txt"))
}

fn start_sidecar_with_timeout(
    model_path: &str,
    ctx_size: Option<u32>,
    timeout: Duration,
) -> Result<String, AppError> {
    let model_path = canonical_existing_file(model_path, "model")?;
    let discovery = discover_llama_cpp();
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

    if let Some(record) = read_backend_sidecar_record()? {
        if process_is_running(record.pid) {
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
        remove_file_if_exists(&backend_sidecar_record_path())?;
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
    configure_sidecar_process(&mut command);
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
    write_backend_sidecar_record(&record)?;
    trace_backend_start("sidecar-record-written");

    let started_at = Instant::now();
    loop {
        trace_backend_start("health-probe-start");
        let health = probe_health(
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
            remove_file_if_exists(&backend_sidecar_record_path())?;
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
            remove_file_if_exists(&backend_sidecar_record_path())?;
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

#[cfg(unix)]
fn configure_sidecar_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_sidecar_process(_command: &mut Command) {}

fn create_log_file(path: &Path) -> Result<File, AppError> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|err| AppError::runtime(format!("log file 생성 실패: {} ({err})", path.display())))
}

fn write_backend_sidecar_record(record: &BackendSidecarRecord) -> Result<(), AppError> {
    let path = backend_sidecar_record_path();
    let parent = path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend sidecar record parent path를 계산하지 못했습니다: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend sidecar record directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    let contents = format!(
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
        record
            .ctx_size
            .map(|value| value.to_string())
            .unwrap_or_default(),
        record.stdout_log.display(),
        record.stderr_log.display(),
        record.started_at_ms
    );
    fs::write(&path, contents).map_err(|err| {
        AppError::runtime(format!(
            "backend sidecar record를 쓰지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

fn read_backend_sidecar_record() -> Result<Option<BackendSidecarRecord>, AppError> {
    let path = backend_sidecar_record_path();
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend sidecar record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_backend_sidecar_record(&contents)
        .map(Some)
        .ok_or_else(|| {
            AppError::blocked(format!(
                "backend sidecar record 형식이 유효하지 않습니다: {}",
                path.display()
            ))
        })
}

fn parse_backend_sidecar_record(contents: &str) -> Option<BackendSidecarRecord> {
    let mut backend_id = None;
    let mut pid = None;
    let mut binary_path = None;
    let mut model_path = None;
    let mut model_sha256 = None;
    let mut model_size_bytes = None;
    let mut backend_release = None;
    let mut binary_sha256 = None;
    let mut mmproj = None;
    let mut host = None;
    let mut port = None;
    let mut ctx_size = None;
    let mut stdout_log = None;
    let mut stderr_log = None;
    let mut started_at_ms = None;

    for line in contents.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "backend_id" => backend_id = Some(value.to_string()),
            "pid" => pid = value.parse::<u32>().ok(),
            "binary_path" => binary_path = Some(PathBuf::from(value)),
            "model_path" => model_path = Some(PathBuf::from(value)),
            "model_sha256" => model_sha256 = Some(value.to_string()),
            "model_size_bytes" => model_size_bytes = value.parse::<u64>().ok(),
            "backend_release" => backend_release = Some(value.to_string()),
            "binary_sha256" => binary_sha256 = Some(value.to_string()),
            "mmproj" => mmproj = Some(value.to_string()),
            "host" => host = Some(value.to_string()),
            "port" => port = value.parse::<u16>().ok(),
            "ctx_size" => {
                ctx_size = if value.is_empty() || value == "model-default" {
                    Some(None)
                } else {
                    let parsed = value.parse::<u32>().ok()?;
                    if parsed == 0 {
                        return None;
                    }
                    Some(Some(parsed))
                };
            }
            "stdout_log" => stdout_log = Some(PathBuf::from(value)),
            "stderr_log" => stderr_log = Some(PathBuf::from(value)),
            "started_at_ms" => started_at_ms = value.parse::<u128>().ok(),
            _ => {}
        }
    }

    Some(BackendSidecarRecord {
        backend_id: backend_id?,
        pid: pid?,
        binary_path: binary_path?,
        model_path: model_path?,
        model_sha256: model_sha256.unwrap_or_else(|| "unknown".to_string()),
        model_size_bytes: model_size_bytes.unwrap_or(0),
        backend_release: backend_release.unwrap_or_else(|| "unknown".to_string()),
        binary_sha256: binary_sha256.unwrap_or_else(|| "unknown".to_string()),
        mmproj: mmproj.unwrap_or_else(|| "unknown".to_string()),
        host: host?,
        port: port?,
        ctx_size: ctx_size.unwrap_or(None),
        stdout_log: stdout_log?,
        stderr_log: stderr_log?,
        started_at_ms: started_at_ms?,
    })
}

fn write_backend_install_record(
    artifact: &BackendReleaseArtifact,
    binary_sha256: &str,
) -> Result<(), AppError> {
    let path = backend_install_record_path();
    let parent = path.parent().ok_or_else(|| {
        AppError::runtime(format!(
            "backend install record parent path를 계산하지 못했습니다: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "backend install record directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;

    let contents = format!(
        "release_tag={}\narchive_sha256={}\nbinary_sha256={}\n",
        LLAMA_CPP_RELEASE.release_tag, artifact.archive_sha256, binary_sha256
    );
    fs::write(&path, contents).map_err(|err| {
        AppError::runtime(format!(
            "backend install record를 쓰지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

fn read_backend_install_record() -> Result<BackendInstallRecord, AppError> {
    let path = backend_install_record_path();
    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "backend install record를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    parse_backend_install_record(&contents).ok_or_else(|| {
        AppError::blocked(format!(
            "backend install record 형식이 유효하지 않습니다: {}",
            path.display()
        ))
    })
}

fn parse_backend_install_record(contents: &str) -> Option<BackendInstallRecord> {
    let mut release_tag = None;
    let mut archive_sha256 = None;
    let mut binary_sha256 = None;

    for line in contents.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "release_tag" => release_tag = Some(value.to_string()),
            "archive_sha256" => archive_sha256 = Some(value.to_string()),
            "binary_sha256" => binary_sha256 = Some(value.to_string()),
            _ => {}
        }
    }

    Some(BackendInstallRecord {
        release_tag: release_tag?,
        archive_sha256: archive_sha256?,
        binary_sha256: binary_sha256?,
    })
}

fn probe_backend_version(discovery: &BackendDiscovery) -> BackendVersionProbe {
    let command = format!("{} --version", discovery.selected_path.display());

    if discovery.selected_source != "managed" {
        return BackendVersionProbe {
            status: "skipped",
            command,
            exit_code: None,
            output: None,
            error: Some(
                "env override backend binary는 doctor에서 자동 실행하지 않습니다.".to_string(),
            ),
        };
    }
    if !discovery.binary_exists || !discovery.binary_is_file {
        return BackendVersionProbe {
            status: "not-run",
            command,
            exit_code: None,
            output: None,
            error: Some("managed backend binary가 없습니다.".to_string()),
        };
    }
    if !discovery.binary_executable {
        return BackendVersionProbe {
            status: "not-run",
            command,
            exit_code: None,
            output: None,
            error: Some("managed backend binary 실행 권한이 없습니다.".to_string()),
        };
    }

    let Some(artifact) = selected_backend_release_artifact(&LLAMA_CPP_RELEASE) else {
        return BackendVersionProbe {
            status: "not-run",
            command,
            exit_code: None,
            output: None,
            error: Some("현재 platform artifact manifest가 없습니다.".to_string()),
        };
    };
    let record = match read_backend_install_record() {
        Ok(record) => record,
        Err(err) => {
            return BackendVersionProbe {
                status: "not-run",
                command,
                exit_code: None,
                output: None,
                error: Some(err.message),
            };
        }
    };
    if record.release_tag != LLAMA_CPP_RELEASE.release_tag
        || record.archive_sha256 != artifact.archive_sha256
    {
        return BackendVersionProbe {
            status: "not-run",
            command,
            exit_code: None,
            output: None,
            error: Some("backend install record가 현재 release manifest와 다릅니다.".to_string()),
        };
    }

    match checksum::sha256_file(&discovery.selected_path) {
        Ok(actual_sha256) if actual_sha256 == record.binary_sha256 => {}
        Ok(_) => {
            return BackendVersionProbe {
                status: "not-run",
                command,
                exit_code: None,
                output: None,
                error: Some(
                    "managed backend binary SHA-256이 install record와 다릅니다.".to_string(),
                ),
            };
        }
        Err(err) => {
            return BackendVersionProbe {
                status: "not-run",
                command,
                exit_code: None,
                output: None,
                error: Some(err.message),
            };
        }
    }

    run_version_command(
        &discovery.selected_path,
        Duration::from_millis(VERSION_TIMEOUT_MS),
    )
}

fn run_version_command(path: &Path, timeout: Duration) -> BackendVersionProbe {
    let command = format!("{} --version", path.display());
    let mut child = match Command::new(path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return BackendVersionProbe {
                status: "error",
                command,
                exit_code: None,
                output: None,
                error: Some(format!("version command 실행 실패: {err}")),
            };
        }
    };

    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                return match child.wait_with_output() {
                    Ok(output) => {
                        let exit_code = output.status.code();
                        BackendVersionProbe {
                            status: if output.status.success() {
                                "ok"
                            } else {
                                "failed"
                            },
                            command,
                            exit_code,
                            output: normalize_version_output(&output.stdout, &output.stderr),
                            error: None,
                        }
                    }
                    Err(err) => BackendVersionProbe {
                        status: "error",
                        command,
                        exit_code: None,
                        output: None,
                        error: Some(format!("version command output 수집 실패: {err}")),
                    },
                };
            }
            Ok(None) if started_at.elapsed() >= timeout => {
                let _ = child.kill();
                let output = child.wait_with_output().ok();
                return BackendVersionProbe {
                    status: "timeout",
                    command,
                    exit_code: output.as_ref().and_then(|output| output.status.code()),
                    output: output.as_ref().and_then(|output| {
                        normalize_version_output(&output.stdout, &output.stderr)
                    }),
                    error: Some(format!(
                        "version command timeout: {} ms",
                        timeout.as_millis()
                    )),
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(err) => {
                let _ = child.kill();
                return BackendVersionProbe {
                    status: "error",
                    command,
                    exit_code: None,
                    output: None,
                    error: Some(format!("version command 상태 확인 실패: {err}")),
                };
            }
        }
    }
}

fn normalize_version_output(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let mut output = String::new();
    output.push_str(&String::from_utf8_lossy(stdout));
    if !stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&String::from_utf8_lossy(stderr));
    }
    let normalized = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.chars().take(500).collect())
    }
}

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    process_running_status(pid).unwrap_or(false)
}

#[cfg(unix)]
fn process_running_status(pid: u32) -> Result<bool, AppError> {
    let Some(pid_arg) = unix_pid_arg(pid) else {
        return Ok(false);
    };
    if process_is_zombie_arg(&pid_arg) {
        return Ok(false);
    }
    Command::new("kill")
        .arg("-0")
        .arg(&pid_arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|err| AppError::runtime(format!("backend process 상태 확인 실패: {err}")))
}

#[cfg(unix)]
fn process_is_zombie_arg(pid_arg: &str) -> bool {
    Command::new("ps")
        .arg("-p")
        .arg(pid_arg)
        .arg("-o")
        .arg("stat=")
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim_start()
                .starts_with('Z')
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn unix_pid_arg(pid: u32) -> Option<String> {
    if pid == 0 || pid > i32::MAX as u32 {
        None
    } else {
        Some(pid.to_string())
    }
}

#[cfg(windows)]
fn process_is_running(pid: u32) -> bool {
    process_running_status(pid).unwrap_or(false)
}

#[cfg(windows)]
fn process_running_status(pid: u32) -> Result<bool, AppError> {
    let output = Command::new("tasklist")
        .arg("/FI")
        .arg(format!("PID eq {pid}"))
        .output()
        .map_err(|err| AppError::runtime(format!("backend process 상태 확인 실패: {err}")))?;
    if !output.status.success() {
        return Err(AppError::runtime(format!(
            "backend process 상태 확인 명령이 실패했습니다: pid={pid}"
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
}

#[cfg(not(any(unix, windows)))]
fn process_is_running(_pid: u32) -> bool {
    false
}

#[cfg(not(any(unix, windows)))]
fn process_running_status(_pid: u32) -> Result<bool, AppError> {
    Err(AppError::blocked(
        "현재 platform에서는 backend process 상태 확인을 지원하지 않습니다.",
    ))
}

#[cfg(unix)]
fn process_command_matches_record(record: &BackendSidecarRecord) -> bool {
    let Some(pid_arg) = unix_pid_arg(record.pid) else {
        return false;
    };
    let Ok(output) = Command::new("ps")
        .arg("-p")
        .arg(pid_arg)
        .arg("-o")
        .arg("command=")
        .output()
    else {
        return false;
    };
    let command = String::from_utf8_lossy(&output.stdout);
    if command.trim().is_empty() {
        return record.backend_id == LLAMA_CPP_BACKEND_ID && record.binary_path.is_file();
    }
    command.contains(&record.binary_path.display().to_string())
        || command.contains(LlamaCppAdapter.binary_name())
        || (record.backend_id == LLAMA_CPP_BACKEND_ID && record.binary_path.is_file())
}

#[cfg(windows)]
fn process_command_matches_record(record: &BackendSidecarRecord) -> bool {
    Command::new("wmic")
        .arg("process")
        .arg("where")
        .arg(format!("processid={}", record.pid))
        .arg("get")
        .arg("CommandLine")
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .contains(&record.binary_path.display().to_string())
        })
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn process_command_matches_record(_record: &BackendSidecarRecord) -> bool {
    false
}

#[cfg(unix)]
fn terminate_process(pid: u32, force: bool) -> Result<(), AppError> {
    let Some(pid_arg) = unix_pid_arg(pid) else {
        return Err(AppError::runtime(format!(
            "backend process 종료 명령이 실패했습니다: invalid unix pid={pid}"
        )));
    };
    let mut command = Command::new("kill");
    if force {
        command.arg("-9");
    }
    let status = command
        .arg(pid_arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| AppError::runtime(format!("backend process 종료 명령 실패: {err}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::runtime(format!(
            "backend process 종료 명령이 실패했습니다: pid={pid}"
        )))
    }
}

#[cfg(windows)]
fn terminate_process(pid: u32, force: bool) -> Result<(), AppError> {
    let mut command = Command::new("taskkill");
    command.arg("/PID").arg(pid.to_string()).arg("/T");
    if force {
        command.arg("/F");
    }
    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| AppError::runtime(format!("backend process 종료 명령 실패: {err}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::runtime(format!(
            "backend process 종료 명령이 실패했습니다: pid={pid}"
        )))
    }
}

#[cfg(not(any(unix, windows)))]
fn terminate_process(_pid: u32, _force: bool) -> Result<(), AppError> {
    Err(AppError::blocked(
        "현재 platform에서는 backend process stop을 지원하지 않습니다.",
    ))
}

fn wait_until_process_stops_checked(pid: u32, timeout: Duration) -> Result<bool, AppError> {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if !process_running_status(pid)? {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(!process_running_status(pid)?)
}

fn selected_backend_release_artifact(
    manifest: &BackendReleaseManifest,
) -> Option<&'static BackendReleaseArtifact> {
    release_artifact_for(manifest, env::consts::OS, env::consts::ARCH)
}

fn release_artifact_for(
    manifest: &BackendReleaseManifest,
    os: &str,
    arch: &str,
) -> Option<&'static BackendReleaseArtifact> {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.os == os && artifact.arch == arch)
}

fn backend_install_blockers(
    manifest: &BackendReleaseManifest,
    artifact: Option<&BackendReleaseArtifact>,
) -> Vec<String> {
    let mut blockers = Vec::new();
    for blocker in manifest.install_blockers {
        push_unique(&mut blockers, *blocker);
    }
    if manifest.release_url.is_empty() {
        push_unique(&mut blockers, "release URL 미확정");
    }
    if manifest.release_api_source.is_empty() {
        push_unique(&mut blockers, "release API source 미확정");
    }
    if manifest.release_tag.is_empty() {
        push_unique(&mut blockers, "release tag 미확정");
    }
    let Some(artifact) = artifact else {
        push_unique(
            &mut blockers,
            format!(
                "지원 platform artifact 미확정 ({}/{})",
                env::consts::OS,
                env::consts::ARCH
            ),
        );
        return blockers;
    };
    if artifact.archive_url.is_empty() {
        push_unique(&mut blockers, "archive URL 미확정");
    }
    if artifact.archive_name.is_empty() {
        push_unique(&mut blockers, "archive name 미확정");
    }
    if !checksum::is_valid_sha256(artifact.archive_sha256) {
        push_unique(&mut blockers, "archive SHA-256 미확정");
    }
    if artifact.archive_size_bytes == 0 {
        push_unique(&mut blockers, "archive file size 미확정");
    }
    if artifact.binary_relative_path.is_empty() {
        push_unique(&mut blockers, "archive 내부 binary path 미확정");
    }
    blockers
}

fn push_unique(values: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
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

fn configured_port(default_port: u16) -> (u16, &'static str) {
    let Some(raw_port) = env::var_os(ENV_BACKEND_PORT) else {
        return (default_port, "default");
    };
    let Some(raw_port) = raw_port.to_str() else {
        return (default_port, "invalid env, default");
    };
    match raw_port.parse::<u16>() {
        Ok(port) if port > 0 => (port, "env override"),
        _ => (default_port, "invalid env, default"),
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn health_status_line_does_not_require_connection_eof() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: keep-alive\r\n";

        assert_eq!(
            first_http_status_line(response).as_deref(),
            Some("HTTP/1.1 200 OK")
        );
        assert_eq!(first_http_status_line(b"HTTP/1.1 200 OK"), None);
    }

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

        let discovery = discover_llama_cpp();

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

        let discovery = discover_llama_cpp();

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

        let discovery = discover_llama_cpp();

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
            BackendArchiveDownloadStatus::CacheHit,
        )
        .unwrap();

        assert!(managed_binary.is_file());
        assert!(is_executable(&managed_binary));
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
        set_executable_bit(&managed_binary).unwrap();
        let binary_sha256 = checksum::sha256_file(&managed_binary).unwrap();
        write_backend_install_record(artifact, &binary_sha256).unwrap();

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
        write_backend_sidecar_record(&record).unwrap();
        let restored = read_backend_sidecar_record().unwrap().unwrap();

        env::remove_var("RPOTATO_DATA_HOME");
        env::remove_var("RPOTATO_PROJECT_ROOT");
        fs::remove_dir_all(root).unwrap();

        assert_eq!(restored.ctx_size, Some(4096));
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
            &backend_generation_cancel_path(),
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
        let marker = fs::read_to_string(backend_generation_cancel_path()).unwrap();

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
        assert!(!backend_generation_record_path().exists());
        assert!(!backend_generation_lock_path().exists());
        assert!(!backend_generation_cancel_path().exists());
        assert!(!backend_generation_terminal_path(&generation.generation_id).exists());
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
            read_backend_generation_record()
                .unwrap()
                .unwrap()
                .generation_id,
            primary.generation_id
        );
        write_generation_terminal_record(&primary.generation_id, "completed", "event-primary")
            .unwrap();
        release_generation_admission(&primary.generation_id).unwrap();
        assert!(backend_generation_record_path().exists());

        let primary_id = primary.generation_id.clone();
        let secondary_id = secondary.generation_id.clone();
        let secondary_acknowledger = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if generation_cancel_requested(&secondary_id).unwrap() {
                    write_generation_terminal_record(&secondary_id, "cancelled", "event-secondary")
                        .unwrap();
                    let both_terminal_while_active = backend_generation_terminal_path(&primary_id)
                        .exists()
                        && backend_generation_terminal_path(&secondary_id).exists()
                        && backend_generation_record_path().exists();
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
        assert!(!backend_generation_record_path().exists());
        assert!(!backend_generation_lock_path().exists());
        assert!(!backend_generation_cancel_path().exists());
        remove_file_if_exists(&backend_generation_terminal_path(&secondary.generation_id)).unwrap();
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
        acquire_backend_generation_lock(&generation).unwrap();
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
        assert!(!backend_generation_record_path().exists());
        assert!(!backend_generation_lock_path().exists());
        assert!(!backend_generation_cancel_path().exists());
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
        let active = read_backend_generation_record().unwrap().unwrap();
        let lock = read_backend_generation_lock_record().unwrap().unwrap();
        assert!(admitted
            .iter()
            .any(|generation| generation.generation_id == active.generation_id));
        assert_eq!(lock.generation_id, active.generation_id);
        release_generation_admission(&admitted[0].generation_id).unwrap();
        assert_eq!(
            read_backend_generation_record()
                .unwrap()
                .unwrap()
                .generation_id,
            active.generation_id
        );
        release_generation_admission(&admitted[1].generation_id).unwrap();
        assert!(!backend_generation_record_path().exists());
        assert!(!backend_generation_lock_path().exists());
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
        write_backend_sidecar_record(&record).unwrap();

        let status = status_report().unwrap();
        let stop = stop_report().unwrap();
        let record_after_stop = read_backend_sidecar_record().unwrap();

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
        set_executable_bit(&backend_script).unwrap();
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
        let record = read_backend_sidecar_record().unwrap();

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
        assert_eq!(unix_pid_arg(0), None);
        assert_eq!(unix_pid_arg(u32::MAX), None);
        assert_eq!(
            unix_pid_arg(i32::MAX as u32),
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
    fn chat_request_body_disables_qwen_thinking() {
        let body = chat_request_body(
            Path::new("Qwen3.5-4B-Q4_K_M.gguf"),
            "감자는 무엇인가?",
            64,
            &CHAT_SAMPLING,
            true,
        );

        assert!(body.contains("\"chat_template_kwargs\":{\"enable_thinking\":false}"));
        assert!(body.contains("\"max_tokens\":64"));
        assert!(body.contains("\"stream\":true"));
        assert!(body.contains("\"include_usage\":true"));
        assert!(body.contains("reasoning trace"));
        assert!(body.contains("감자는 무엇인가?"));
    }

    #[test]
    fn chat_request_body_does_not_send_qwen_option_to_gemma() {
        let body = chat_request_body(
            Path::new("gemma-4-E4B_q4_0-it.gguf"),
            "감자",
            64,
            &CHAT_SAMPLING,
            true,
        );

        assert!(!body.contains("chat_template_kwargs"));
        assert!(body.contains("\"temperature\":0.1"));
    }

    #[test]
    fn parses_chat_completion_response_content_and_usage() {
        let body = r#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"감자는 땅속에서 자라는 식물입니다."}}],"usage":{"completion_tokens":14,"prompt_tokens":26,"total_tokens":40}}"#;

        let completion = parse_chat_completion_response(body).unwrap();

        assert_eq!(completion.content, "감자는 땅속에서 자라는 식물입니다.");
        assert_eq!(completion.finish_reason, "stop");
        assert_eq!(completion.prompt_tokens, Some(26));
        assert_eq!(completion.completion_tokens, Some(14));
        assert_eq!(completion.total_tokens, Some(40));
    }

    #[test]
    fn model_id_comes_from_model_file_stem() {
        let model_id = model_id_from_path(Path::new("/tmp/Qwen3.5-4B-Q4_K_M.gguf"));

        assert_eq!(model_id, "Qwen3.5-4B-Q4_K_M");
    }

    #[test]
    fn strips_closed_reasoning_trace_without_showing_it() {
        let (content, stripped) = strip_reasoning_trace(
            "\n<think>\n내부 추론입니다.\n</think>\n\n감자는 땅속에서 자랍니다.",
        );

        assert!(stripped);
        assert_eq!(content.trim(), "감자는 땅속에서 자랍니다.");
        assert!(!content.contains("내부 추론"));
    }

    #[test]
    fn strips_unclosed_reasoning_trace_to_empty() {
        let (content, stripped) = strip_reasoning_trace("<think>\nThinking Process:");

        assert!(stripped);
        assert!(content.trim().is_empty());
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
