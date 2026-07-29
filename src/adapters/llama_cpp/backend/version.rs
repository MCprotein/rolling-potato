//! Managed backend version validation and bounded process probing.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{BackendDiscovery, BackendVersionProbe};
use crate::adapters::llama_cpp::install::{self, selected_release_artifact, LLAMA_CPP_RELEASE};
use crate::foundation::integrity as checksum;

const VERSION_TIMEOUT_MS: u64 = 5_000;

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
