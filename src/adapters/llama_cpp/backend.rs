use std::env;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adapters::filesystem::layout as paths;
use crate::foundation::serialization::escape_string_content;
use crate::runtime_core::inference::backend::{BackendAdapter, BackendChatSampling};

pub(crate) const LLAMA_CPP_BACKEND_ID: &str = "llama.cpp";
pub(crate) const DEFAULT_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_PORT: u16 = 17842;
pub(crate) const ENV_BACKEND_PATH: &str = "RPOTATO_BACKEND_LLAMA_CPP_PATH";
pub(crate) const ENV_BACKEND_PORT: &str = "RPOTATO_BACKEND_PORT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LlamaCppAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendDiscovery {
    pub(crate) adapter_id: &'static str,
    pub(crate) binary_name: &'static str,
    pub(crate) managed_path: PathBuf,
    pub(crate) selected_path: PathBuf,
    pub(crate) selected_source: &'static str,
    pub(crate) override_path: Option<PathBuf>,
    pub(crate) binary_exists: bool,
    pub(crate) binary_is_file: bool,
    pub(crate) binary_executable: bool,
    pub(crate) host: &'static str,
    pub(crate) port: u16,
    pub(crate) port_source: &'static str,
    pub(crate) health_url: String,
}

pub(crate) struct HealthProbe {
    pub(crate) status: &'static str,
    pub(crate) tcp_connected: bool,
    pub(crate) http_status_line: Option<String>,
    pub(crate) error: Option<String>,
}

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

pub(crate) fn discover() -> BackendDiscovery {
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

pub(crate) fn probe_health(host: &str, port: u16, timeout: Duration) -> HealthProbe {
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
        http_status_line: Some(status_line),
        error: None,
    }
}

pub(crate) fn first_http_status_line(response: &[u8]) -> Option<String> {
    let end = response.iter().position(|byte| *byte == b'\n')?;
    let line = response[..end]
        .strip_suffix(b"\r")
        .unwrap_or(&response[..end]);
    std::str::from_utf8(line).ok().map(str::to_string)
}

pub(crate) fn chat_request_body(
    model_path: &Path,
    prompt: &str,
    max_tokens: u32,
    sampling: &BackendChatSampling,
    stream: bool,
) -> String {
    let system_prompt = "사용자에게 보이는 최종 답변만 한국어로 작성합니다. reasoning trace, <think> 태그, 내부 추론은 출력하지 않습니다.";
    let model_id = model_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-model");
    let template_options = if model_id.to_ascii_lowercase().starts_with("qwen") {
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
        escape_string_content(system_prompt),
        escape_string_content(prompt),
        max_tokens,
        sampling.temperature,
        sampling.top_p,
        template_options,
        stream_options
    )
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
pub(crate) fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub(crate) fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn chat_request_disables_qwen_thinking_and_enables_usage_stream() {
        let body = chat_request_body(
            Path::new("Qwen3.5-4B-Q4_K_M.gguf"),
            "감자는 무엇인가?",
            64,
            &BackendChatSampling {
                temperature: 0.1,
                top_p: 0.8,
            },
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
    fn chat_request_omits_qwen_option_for_other_models() {
        let body = chat_request_body(
            Path::new("gemma-4-E4B_q4_0-it.gguf"),
            "감자",
            64,
            &BackendChatSampling {
                temperature: 0.1,
                top_p: 0.8,
            },
            true,
        );

        assert!(!body.contains("chat_template_kwargs"));
        assert!(body.contains("\"temperature\":0.1"));
    }
}
