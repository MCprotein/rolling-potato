use std::env;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::app::AppError;
use crate::paths;
use crate::{checksum, state};

const LLAMA_CPP_BACKEND_ID: &str = "llama.cpp";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 17842;
const HEALTH_TIMEOUT_MS: u64 = 500;
const ENV_BACKEND_PATH: &str = "RPOTATO_BACKEND_LLAMA_CPP_PATH";
const ENV_BACKEND_PORT: &str = "RPOTATO_BACKEND_PORT";

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
    release_url: Option<&'static str>,
    archive_name: Option<&'static str>,
    archive_sha256: Option<&'static str>,
    archive_size_bytes: Option<u64>,
    install_blockers: &'static [&'static str],
}

const LLAMA_CPP_RELEASE: BackendReleaseManifest = BackendReleaseManifest {
    id: LLAMA_CPP_BACKEND_ID,
    upstream_source: "https://github.com/ggml-org/llama.cpp",
    license: "MIT",
    license_source: "https://github.com/ggml-org/llama.cpp/blob/master/LICENSE",
    license_checked_at: "2026-06-29",
    release_url: None,
    archive_name: None,
    archive_sha256: None,
    archive_size_bytes: None,
    install_blockers: &["검증된 llama.cpp release manifest 미확정"],
};

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
        "backend 진단\n- adapter: {}\n- binary name: {}\n- managed binary: {}\n- selected binary: {}\n- selected source: {}\n- override env {}: {}\n- binary exists: {}\n- binary is file: {}\n- executable bit: {}\n- host: {}\n- port: {} ({})\n- health URL: {}\n- install status: {}\n- version detection: not-run, unknown binary execution은 아직 doctor에서 수행하지 않습니다.\n- install gate: 검증된 llama.cpp release URL과 checksum manifest가 들어오기 전까지 다운로드/설치는 차단합니다.",
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
        install_status
    )
}

pub fn install_plan_report() -> String {
    let discovery = discover_llama_cpp();
    let blockers = backend_install_blockers(&LLAMA_CPP_RELEASE);
    let install_status = if blockers.is_empty() {
        "ready"
    } else {
        "blocked"
    };

    format!(
        "backend install plan\n- id: {}\n- status: {}\n- upstream source: {}\n- license: {}\n- license source: {}\n- license checked-at: {}\n- release URL: {}\n- archive name: {}\n- archive size bytes: {}\n- archive sha256: {}\n- managed binary: {}\n- selected binary: {}\n- selected source: {}\n- download path: {}\n- blockers: {}\n- 동작: 실제 backend 다운로드 전 release URL, checksum, size, license를 사용자에게 표시해야 합니다.",
        LLAMA_CPP_RELEASE.id,
        install_status,
        LLAMA_CPP_RELEASE.upstream_source,
        LLAMA_CPP_RELEASE.license,
        LLAMA_CPP_RELEASE.license_source,
        LLAMA_CPP_RELEASE.license_checked_at,
        LLAMA_CPP_RELEASE.release_url.unwrap_or("미확정"),
        LLAMA_CPP_RELEASE.archive_name.unwrap_or("미확정"),
        LLAMA_CPP_RELEASE
            .archive_size_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "미확정".to_string()),
        LLAMA_CPP_RELEASE.archive_sha256.unwrap_or("미확정"),
        discovery.managed_path.display(),
        discovery.selected_path.display(),
        discovery.selected_source,
        paths::downloads_dir()
            .join(LLAMA_CPP_RELEASE.archive_name.unwrap_or("llama.cpp.archive.part"))
            .display(),
        display_vec(&blockers)
    )
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

fn backend_install_blockers(manifest: &BackendReleaseManifest) -> Vec<String> {
    let mut blockers = Vec::new();
    for blocker in manifest.install_blockers {
        push_unique(&mut blockers, blocker);
    }
    if manifest.release_url.is_none() {
        push_unique(&mut blockers, "release URL 미확정");
    }
    if manifest.archive_name.is_none() {
        push_unique(&mut blockers, "archive name 미확정");
    }
    if manifest.archive_sha256.is_none() {
        push_unique(&mut blockers, "archive SHA-256 미확정");
    }
    if manifest.archive_size_bytes.is_none() {
        push_unique(&mut blockers, "archive file size 미확정");
    }
    blockers
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
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
    fn install_plan_is_blocked_without_release_manifest() {
        let report = install_plan_report();
        assert!(report.contains("status: blocked"));
        assert!(report.contains("archive SHA-256"));
    }

    #[test]
    fn health_check_report_is_diagnostic_not_process_start() {
        let report = health_check_report();
        assert!(report.contains("backend health check"));
        assert!(report.contains("health URL"));
        assert!(report.contains("timeout ms"));
    }
}
