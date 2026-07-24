use std::env;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::adapters::filesystem::layout as paths;
use crate::adapters::llama_cpp::install::{self, selected_release_artifact, LLAMA_CPP_RELEASE};
use crate::foundation::integrity as checksum;
use crate::foundation::serialization::escape_string_content;
use crate::runtime_core::inference::backend::{
    BackendAdapter, BackendChatInput, BackendChatSampling,
};

pub(crate) const LLAMA_CPP_BACKEND_ID: &str = "llama.cpp";
pub(crate) const DEFAULT_HOST: &str = "127.0.0.1";
pub(crate) const DEFAULT_PORT: u16 = 17842;
pub(crate) const ENV_BACKEND_PATH: &str = "RPOTATO_BACKEND_LLAMA_CPP_PATH";
pub(crate) const ENV_BACKEND_PORT: &str = "RPOTATO_BACKEND_PORT";
const VERSION_TIMEOUT_MS: u64 = 5_000;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendVersionProbe {
    pub(crate) status: &'static str,
    pub(crate) command: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) output: Option<String>,
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

#[cfg(test)]
pub(crate) fn chat_request_body(
    model_path: &Path,
    prompt: &str,
    max_tokens: u32,
    sampling: &BackendChatSampling,
    stream: bool,
) -> String {
    chat_request_body_for_input(
        model_path,
        &BackendChatInput::text_for_user(prompt, prompt),
        max_tokens,
        sampling,
        stream,
    )
}

pub(crate) fn chat_request_body_for_input(
    model_path: &Path,
    input: &BackendChatInput,
    max_tokens: u32,
    sampling: &BackendChatSampling,
    stream: bool,
) -> String {
    let system_prompt = if input.response_language.allows_non_korean() {
        "사용자가 명시적으로 요청한 출력 언어를 따릅니다. reasoning trace, <think> 태그, 내부 추론은 출력하지 않습니다."
    } else {
        "기본 답변은 자연스러운 한국어로 작성하고, 코드·수식·URL·고유명사는 필요한 원문 표기를 유지합니다. reasoning trace, <think> 태그, 내부 추론은 출력하지 않습니다."
    };
    let model_id = model_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown-model");
    let normalized_model_id = model_id.to_ascii_lowercase();
    let template_options =
        if normalized_model_id.starts_with("qwen") || normalized_model_id.starts_with("gemma-4") {
            ",\"chat_template_kwargs\":{\"enable_thinking\":false}"
        } else {
            ""
        };
    let stream_options = if stream {
        ",\"stream\":true,\"stream_options\":{\"include_usage\":true}"
    } else {
        ""
    };
    let user_content = if input.images.is_empty() {
        format!("\"{}\"", escape_string_content(&input.text))
    } else {
        let mut parts = Vec::with_capacity(input.images.len() + 1);
        if !input.text.trim().is_empty() {
            parts.push(format!(
                "{{\"type\":\"text\",\"text\":\"{}\"}}",
                escape_string_content(&input.text)
            ));
        }
        parts.extend(input.images.iter().map(|image| {
            format!(
                "{{\"type\":\"image_url\",\"image_url\":{{\"url\":\"data:{};base64,{}\"}}}}",
                escape_string_content(&image.mime_type),
                encode_base64(&image.bytes)
            )
        }));
        format!("[{}]", parts.join(","))
    };
    format!(
        "{{\"messages\":[{{\"role\":\"system\",\"content\":\"{}\"}},{{\"role\":\"user\",\"content\":{}}}],\"max_tokens\":{},\"temperature\":{},\"top_p\":{}{}{}}}",
        escape_string_content(system_prompt),
        user_content,
        max_tokens,
        sampling.temperature,
        sampling.top_p,
        template_options,
        stream_options
    )
}

fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(third & 0b11_1111) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

pub(crate) fn probe_version(discovery: &BackendDiscovery) -> BackendVersionProbe {
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

    let Some(artifact) = selected_release_artifact(&LLAMA_CPP_RELEASE) else {
        return BackendVersionProbe {
            status: "not-run",
            command,
            exit_code: None,
            output: None,
            error: Some("현재 platform artifact manifest가 없습니다.".to_string()),
        };
    };
    let record = match install::read_install_record() {
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

pub(crate) fn sidecar_command(
    binary_path: &Path,
    model_path: &Path,
    mmproj_path: Option<&Path>,
    host: &str,
    port: u16,
    ctx_size: Option<u32>,
) -> Command {
    let mut command = Command::new(binary_path);
    command
        .arg("--model")
        .arg(model_path)
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(port.to_string());
    if let Some(mmproj_path) = mmproj_path {
        command.arg("--mmproj").arg(mmproj_path);
    }
    if let Some(ctx_size) = ctx_size {
        command.arg("--ctx-size").arg(ctx_size.to_string());
    }
    command
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
    use crate::runtime_core::inference::backend::BackendChatImage;

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
    fn chat_request_disables_gemma_4_thinking() {
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

        assert!(body.contains("\"chat_template_kwargs\":{\"enable_thinking\":false}"));
        assert!(body.contains("\"temperature\":0.1"));
    }

    #[test]
    fn chat_request_omits_thinking_option_for_unrecognized_models() {
        let body = chat_request_body(
            Path::new("custom-model.gguf"),
            "감자",
            64,
            &BackendChatSampling {
                temperature: 0.1,
                top_p: 0.8,
            },
            true,
        );

        assert!(!body.contains("chat_template_kwargs"));
    }

    #[test]
    fn multimodal_request_uses_openai_image_content_parts() {
        let input = BackendChatInput {
            text: "이 이미지의 오류를 설명해줘".to_string(),
            images: vec![BackendChatImage {
                display_name: "screen.png".to_string(),
                mime_type: "image/png".to_string(),
                sha256: "a".repeat(64),
                bytes: b"abc".to_vec(),
            }],
            response_language:
                crate::runtime_core::inference::backend::ResponseLanguage::KoreanDefault,
        };

        let body = chat_request_body_for_input(
            Path::new("gemma-4-E4B_q4_0-it.gguf"),
            &input,
            64,
            &BackendChatSampling {
                temperature: 0.1,
                top_p: 0.8,
            },
            true,
        );

        assert!(body.contains("\"type\":\"text\""));
        assert!(body.contains("\"type\":\"image_url\""));
        assert!(body.contains("data:image/png;base64,YWJj"));
        assert!(!body.contains("screen.png"));
    }

    #[test]
    fn base64_encoder_matches_rfc_4648_padding_vectors() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn request_system_policy_respects_an_explicit_output_language() {
        let body = chat_request_body(
            Path::new("model.gguf"),
            "이 문장을 영어로 번역해줘",
            32,
            &BackendChatSampling {
                temperature: 0.1,
                top_p: 0.8,
            },
            false,
        );

        assert!(body.contains("명시적으로 요청한 출력 언어"));
        assert!(!body.contains("기본 답변은 자연스러운 한국어"));
    }

    #[test]
    fn vision_ready_sidecar_enters_llama_server_with_mmproj() {
        let command = sidecar_command(
            Path::new("/bin/llama-server"),
            Path::new("/models/model.gguf"),
            Some(Path::new("/models/mmproj.gguf")),
            "127.0.0.1",
            17842,
            Some(4096),
        );
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            [
                "--model",
                "/models/model.gguf",
                "--host",
                "127.0.0.1",
                "--port",
                "17842",
                "--mmproj",
                "/models/mmproj.gguf",
                "--ctx-size",
                "4096"
            ]
        );
    }

    #[test]
    fn text_ready_sidecar_does_not_claim_mmproj() {
        let command = sidecar_command(
            Path::new("/bin/llama-server"),
            Path::new("/models/model.gguf"),
            None,
            "127.0.0.1",
            17842,
            None,
        );
        let args = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(!args.iter().any(|value| value == "--mmproj"));
    }
}
