use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::app::AppError;
use crate::paths;
use crate::{checksum, ledger, state};

const LLAMA_CPP_BACKEND_ID: &str = "llama.cpp";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 17842;
const HEALTH_TIMEOUT_MS: u64 = 500;
const ENV_BACKEND_PATH: &str = "RPOTATO_BACKEND_LLAMA_CPP_PATH";
const ENV_BACKEND_PORT: &str = "RPOTATO_BACKEND_PORT";
const DOWNLOAD_BUFFER_BYTES: usize = 64 * 1024;
const VERSION_TIMEOUT_MS: u64 = 5_000;
const STARTUP_TIMEOUT_MS: u64 = 60_000;
const STOP_TIMEOUT_MS: u64 = 5_000;
const CHAT_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_CHAT_MAX_TOKENS: u32 = 128;
const QWEN_NON_THINKING_SOURCE: &str =
    "https://huggingface.co/Qwen/Qwen3.5-4B#instruct-or-non-thinking-mode";

pub trait BackendAdapter {
    fn id(&self) -> &'static str;
    fn binary_name(&self) -> &'static str;
    fn managed_binary_path(&self) -> PathBuf;
    fn default_host(&self) -> &'static str;
    fn default_port(&self) -> u16;
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LlamaCppAdapter;

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
    host: String,
    port: u16,
    ctx_size: Option<u32>,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
    started_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq)]
struct BackendChatCompletion {
    content: String,
    finish_reason: String,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
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
    license_source: "https://github.com/ggml-org/llama.cpp/blob/master/LICENSE",
    license_checked_at: "2026-06-29",
    release_tag: "b9878",
    release_url: "https://github.com/ggml-org/llama.cpp/releases/tag/b9878",
    release_api_source: "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest",
    release_checked_at: "2026-07-06",
    artifacts: &LLAMA_CPP_RELEASE_ARTIFACTS,
    install_blockers: &[],
};

const LLAMA_CPP_RELEASE_ARTIFACTS: [BackendReleaseArtifact; 6] = [
    BackendReleaseArtifact {
        os: "macos",
        arch: "aarch64",
        archive_name: "llama-b9878-bin-macos-arm64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9878/llama-b9878-bin-macos-arm64.tar.gz",
        archive_sha256: "3c18b48c3d4e4fb6e66c8188c6ac06849d9da6919511c061e310e18682432b57",
        archive_size_bytes: 11_136_305,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "macos",
        arch: "x86_64",
        archive_name: "llama-b9878-bin-macos-x64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9878/llama-b9878-bin-macos-x64.tar.gz",
        archive_sha256: "4b62fc570e58984517bb91f12143b348ffdca6810b1fbbce781a50ec53cae081",
        archive_size_bytes: 11_451_412,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "linux",
        arch: "aarch64",
        archive_name: "llama-b9878-bin-ubuntu-arm64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9878/llama-b9878-bin-ubuntu-arm64.tar.gz",
        archive_sha256: "f45b9dc866e939e975ac49345e0ddd302450de637b49648bfaf7ac2c2d20b1d5",
        archive_size_bytes: 12_865_159,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "linux",
        arch: "x86_64",
        archive_name: "llama-b9878-bin-ubuntu-x64.tar.gz",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9878/llama-b9878-bin-ubuntu-x64.tar.gz",
        archive_sha256: "fa52c1bdc6a17f28bfeaad28ca6783ff94cf85f36dca4a4bb2d9c7e8687c007b",
        archive_size_bytes: 15_866_030,
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    },
    BackendReleaseArtifact {
        os: "windows",
        arch: "aarch64",
        archive_name: "llama-b9878-bin-win-cpu-arm64.zip",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9878/llama-b9878-bin-win-cpu-arm64.zip",
        archive_sha256: "a7f3307a62b2fdf367d62302217fdcd0a2f2723ed0fd55052f8a880b33e14fe5",
        archive_size_bytes: 11_380_283,
        archive_kind: BackendArchiveKind::Zip,
        binary_relative_path: "llama-server.exe",
    },
    BackendReleaseArtifact {
        os: "windows",
        arch: "x86_64",
        archive_name: "llama-b9878-bin-win-cpu-x64.zip",
        archive_url: "https://github.com/ggml-org/llama.cpp/releases/download/b9878/llama-b9878-bin-win-cpu-x64.zip",
        archive_sha256: "66e0e038c73aedefeed54c92ebfc3e7b8531fbf0b49ad6c21e50d93afd7e224e",
        archive_size_bytes: 17_482_794,
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

    Ok(format!(
        "backend status\n- status: {}\n- backend: {}\n- pid: {}\n- binary: {}\n- model: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- health: {}\n- health error: {}\n- stdout log: {}\n- stderr log: {}\n- sidecar record: {}",
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

    terminate_process(record.pid, false)?;
    let stopped = wait_until_process_stops(record.pid, Duration::from_millis(STOP_TIMEOUT_MS));
    if !stopped {
        terminate_process(record.pid, true)?;
        if !wait_until_process_stops(record.pid, Duration::from_millis(STOP_TIMEOUT_MS)) {
            return Err(AppError::blocked(format!(
                "backend stop 실패\n- pid: {}\n- 이유: graceful/force 종료 후에도 process가 남아 있습니다.",
                record.pid
            )));
        }
    }
    remove_file_if_exists(&backend_sidecar_record_path())?;
    let event_id = state::record_event(
        "backend.sidecar.stop.completed",
        "backend sidecar 종료 완료",
        &format!(
            "pid={} binary={} command_matched={}",
            record.pid,
            record.binary_path.display(),
            command_matched
        ),
    )?;

    Ok(format!(
        "backend stop\n- status: stopped\n- pid: {}\n- command matched: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
        record.pid,
        command_matched,
        record.stdout_log.display(),
        record.stderr_log.display(),
        event_id
    ))
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

pub fn chat_report(prompt: &str, max_tokens: Option<u32>) -> Result<String, AppError> {
    if prompt.trim().is_empty() {
        return Err(AppError::usage(
            "backend chat은 비어 있지 않은 --prompt <text> 값이 필요합니다.",
        ));
    }
    let max_tokens = max_tokens.unwrap_or(DEFAULT_CHAT_MAX_TOKENS);
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

    let started_at = Instant::now();
    let completion = request_chat_completion(&record, prompt, max_tokens)?;
    let (display_content, had_reasoning_trace) = strip_reasoning_trace(&completion.content);
    let display_content = display_content.trim().to_string();
    let guard_status = if had_reasoning_trace {
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
            "pid={} backend={} prompt_chars={} output_chars={} max_tokens={} finish_reason={} guard_status={} prompt_tokens={} completion_tokens={} total_tokens={} elapsed_ms={}",
            record.pid,
            record.backend_id,
            prompt.chars().count(),
            display_content.chars().count(),
            max_tokens,
            completion.finish_reason,
            guard_status,
            display_optional_u32(completion.prompt_tokens),
            display_optional_u32(completion.completion_tokens),
            display_optional_u32(completion.total_tokens),
            started_at.elapsed().as_millis()
        ),
    )?;

    if display_content.is_empty() {
        return Err(AppError::blocked(format!(
            "backend chat 차단\n- 이유: reasoning trace 제거 후 표시 가능한 응답이 없습니다.\n- endpoint: /v1/chat/completions\n- thinking mode: disabled via chat_template_kwargs.enable_thinking=false\n- guard: {}\n- finish reason: {}\n- ledger event: {}",
            guard_status, completion.finish_reason, event_id
        )));
    }

    Ok(format!(
        "backend chat\n- status: completed\n- backend: {}\n- pid: {}\n- endpoint: /v1/chat/completions\n- thinking mode: disabled via chat_template_kwargs.enable_thinking=false\n- non-thinking source: {}\n- prompt chars: {}\n- max tokens: {}\n- finish reason: {}\n- guard: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- elapsed ms: {}\n- ledger event: {}\n- response:\n{}",
        record.backend_id,
        record.pid,
        QWEN_NON_THINKING_SOURCE,
        prompt.chars().count(),
        max_tokens,
        completion.finish_reason,
        guard_status,
        display_optional_u32(completion.prompt_tokens),
        display_optional_u32(completion.completion_tokens),
        display_optional_u32(completion.total_tokens),
        started_at.elapsed().as_millis(),
        event_id,
        display_content
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

    let mut response = String::new();
    if let Err(err) = stream.read_to_string(&mut response) {
        return HealthProbe {
            status: "unhealthy",
            tcp_connected: true,
            http_status_line: None,
            error: Some(format!("health response read 실패: {err}")),
        };
    }

    let status_line = response.lines().next().unwrap_or("").to_string();
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

fn request_chat_completion(
    record: &BackendSidecarRecord,
    prompt: &str,
    max_tokens: u32,
) -> Result<BackendChatCompletion, AppError> {
    let body = chat_request_body(prompt, max_tokens);
    let response_body = post_json(
        &record.host,
        record.port,
        "/v1/chat/completions",
        &body,
        Duration::from_millis(CHAT_TIMEOUT_MS),
    )?;
    parse_chat_completion_response(&response_body).ok_or_else(|| {
        AppError::blocked(
            "backend chat 응답을 해석하지 못했습니다. content 또는 usage field가 예상 형식과 다릅니다.",
        )
    })
}

fn chat_request_body(prompt: &str, max_tokens: u32) -> String {
    let system_prompt = "사용자에게 보이는 최종 답변만 한국어로 작성합니다. reasoning trace, <think> 태그, 내부 추론은 출력하지 않습니다.";
    format!(
        "{{\"messages\":[{{\"role\":\"system\",\"content\":\"{}\"}},{{\"role\":\"user\",\"content\":\"{}\"}}],\"max_tokens\":{},\"temperature\":0.1,\"top_p\":0.8,\"chat_template_kwargs\":{{\"enable_thinking\":false}}}}",
        ledger::json_string(system_prompt),
        ledger::json_string(prompt),
        max_tokens
    )
}

fn post_json(
    host: &str,
    port: u16,
    path: &str,
    body: &str,
    timeout: Duration,
) -> Result<String, AppError> {
    let address = format!("{host}:{port}");
    let mut addresses = address.to_socket_addrs().map_err(|err| {
        AppError::runtime(format!("backend address resolve 실패: {address} ({err})"))
    })?;
    let socket_addr = addresses
        .next()
        .ok_or_else(|| AppError::runtime(format!("backend address 없음: {address}")))?;
    let mut stream = TcpStream::connect_timeout(&socket_addr, timeout)
        .map_err(|err| AppError::runtime(format!("backend 연결 실패: {socket_addr} ({err})")))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| AppError::runtime(format!("backend request write 실패: {err}")))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| AppError::runtime(format!("backend response read 실패: {err}")))?;
    let status_line = response.lines().next().unwrap_or("");
    let response_body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    if !(status_line.contains(" 200 ") || status_line.ends_with(" 200")) {
        return Err(AppError::blocked(format!(
            "backend request 실패\n- endpoint: {}\n- status: {}\n- body preview: {}",
            path,
            if status_line.is_empty() {
                "없음"
            } else {
                status_line
            },
            preview_for_error(&response_body)
        )));
    }
    Ok(response_body)
}

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

fn preview_for_error(value: &str) -> String {
    let compact = value.replace(['\r', '\n', '\t'], " ");
    let preview: String = compact.chars().take(200).collect();
    if compact.chars().count() > 200 {
        format!("{preview}...")
    } else if preview.is_empty() {
        "없음".to_string()
    } else {
        preview
    }
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
        .header("User-Agent", "rpotato/0.1.0")
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

fn backend_install_record_path() -> PathBuf {
    paths::backends_dir()
        .join("llama.cpp")
        .join("install-record.txt")
}

fn backend_sidecar_record_path() -> PathBuf {
    paths::state_dir().join("backend-llama.cpp-sidecar.txt")
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
            return Ok(format!(
                "backend start\n- status: already-running\n- pid: {}\n- binary: {}\n- model: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- stdout log: {}\n- stderr log: {}",
                record.pid,
                record.binary_path.display(),
                record.model_path.display(),
                record.host,
                record.port,
                display_optional_u32(record.ctx_size),
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

    let record = BackendSidecarRecord {
        backend_id: discovery.adapter_id.to_string(),
        pid: child.id(),
        binary_path,
        model_path,
        host: discovery.host.to_string(),
        port: discovery.port,
        ctx_size,
        stdout_log,
        stderr_log,
        started_at_ms: now_ms(),
    };
    write_backend_sidecar_record(&record)?;

    let started_at = Instant::now();
    loop {
        let health = probe_health(
            &record.host,
            record.port,
            Duration::from_millis(HEALTH_TIMEOUT_MS),
        );
        if health.status == "healthy" {
            let event_id = state::record_event(
                "backend.sidecar.start.completed",
                "backend sidecar 시작 완료",
                &format!(
                    "pid={} binary={} model={} port={} ctx_size={} startup_ms={} stdout_log={} stderr_log={}",
                    record.pid,
                    record.binary_path.display(),
                    record.model_path.display(),
                    record.port,
                    display_optional_u32(record.ctx_size),
                    started_at.elapsed().as_millis(),
                    record.stdout_log.display(),
                    record.stderr_log.display()
                ),
            )?;
            return Ok(format!(
                "backend start\n- status: running\n- pid: {}\n- binary: {}\n- model: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- startup ms: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
                record.pid,
                record.binary_path.display(),
                record.model_path.display(),
                record.host,
                record.port,
                display_optional_u32(record.ctx_size),
                started_at.elapsed().as_millis(),
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
        "backend_id={}\npid={}\nbinary_path={}\nmodel_path={}\nhost={}\nport={}\nctx_size={}\nstdout_log={}\nstderr_log={}\nstarted_at_ms={}\n",
        record.backend_id,
        record.pid,
        record.binary_path.display(),
        record.model_path.display(),
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
    if process_is_zombie(pid) {
        return false;
    }
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn process_is_zombie(pid: u32) -> bool {
    Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
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

#[cfg(windows)]
fn process_is_running(pid: u32) -> bool {
    Command::new("tasklist")
        .arg("/FI")
        .arg(format!("PID eq {pid}"))
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(not(any(unix, windows)))]
fn process_is_running(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn process_command_matches_record(record: &BackendSidecarRecord) -> bool {
    let Ok(output) = Command::new("ps")
        .arg("-p")
        .arg(record.pid.to_string())
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
    let mut command = Command::new("kill");
    if force {
        command.arg("-9");
    }
    let status = command
        .arg(pid.to_string())
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

fn wait_until_process_stops(pid: u32, timeout: Duration) -> bool {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if !process_is_running(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    !process_is_running(pid)
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
    fn release_manifest_has_source_backed_supported_artifact() {
        let artifact = release_artifact_for(&LLAMA_CPP_RELEASE, "macos", "aarch64")
            .expect("macOS arm64 backend artifact should be recorded");

        assert!(artifact
            .archive_url
            .starts_with("https://github.com/ggml-org/llama.cpp/releases/download/b9878/"));
        assert!(checksum::is_valid_sha256(artifact.archive_sha256));
        assert!(artifact.archive_size_bytes > 0);
        assert_eq!(artifact.archive_kind, BackendArchiveKind::TarGz);
        assert_eq!(
            backend_install_blockers(&LLAMA_CPP_RELEASE, Some(artifact)),
            Vec::<String>::new()
        );
    }

    #[test]
    fn install_plan_uses_current_platform_manifest_when_supported() {
        let report = install_plan_report();

        if selected_backend_release_artifact(&LLAMA_CPP_RELEASE).is_some() {
            assert!(report.contains("status: ready"));
            assert!(report.contains("archive sha256: "));
            assert!(report.contains("release tag: b9878"));
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
        fs::write(
            &managed_binary,
            "#!/bin/sh\necho 'llama.cpp fake version b9878'\n",
        )
        .unwrap();
        set_executable_bit(&managed_binary).unwrap();
        let binary_sha256 = checksum::sha256_file(&managed_binary).unwrap();
        write_backend_install_record(artifact, &binary_sha256).unwrap();

        let report = doctor_report();

        env::remove_var("RPOTATO_DATA_HOME");
        fs::remove_dir_all(root).unwrap();
        assert!(report.contains("version detection: ok"));
        assert!(report.contains("llama.cpp fake version b9878"));
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
            "#!/bin/sh\necho 'booting stdout'\necho 'booting stderr' >&2\nsleep 10\n",
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

    #[test]
    fn health_check_report_is_diagnostic_not_process_start() {
        let report = health_check_report();
        assert!(report.contains("backend health check"));
        assert!(report.contains("health URL"));
        assert!(report.contains("timeout ms"));
    }

    #[test]
    fn chat_request_body_disables_qwen_thinking() {
        let body = chat_request_body("감자는 무엇인가?", 64);

        assert!(body.contains("\"chat_template_kwargs\":{\"enable_thinking\":false}"));
        assert!(body.contains("\"max_tokens\":64"));
        assert!(body.contains("reasoning trace"));
        assert!(body.contains("감자는 무엇인가?"));
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
