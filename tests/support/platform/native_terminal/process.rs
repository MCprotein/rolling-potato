use super::*;

pub(super) fn run_bounded_command(
    command: &mut Command,
    label: &str,
    data: &std::path::Path,
) -> Output {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "rpotato-native-terminal-output-{}-{nonce}",
        std::process::id()
    ));
    let stdout_path = base.with_extension("stdout");
    let stderr_path = base.with_extension("stderr");
    command
        .stdout(Stdio::from(std::fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr_path).unwrap()));
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + FIXTURE_COMMAND_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let status = child.wait().unwrap();
                let output = captured_command_output(&stdout_path, &stderr_path, status);
                panic!(
                    "native fixture command timeout after {:?}: {label}\nstdout={}\nstderr={}\n{}",
                    FIXTURE_COMMAND_TIMEOUT,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                    backend_failure_diagnostics(data),
                );
            }
            Err(error) => panic!("native fixture command wait failed: {label}: {error}"),
        }
    };
    captured_command_output(&stdout_path, &stderr_path, status)
}

pub(super) fn backend_failure_diagnostics(data: &std::path::Path) -> String {
    let mut diagnostics = Vec::new();
    let logs = data.join("logs");
    if let Ok(entries) = std::fs::read_dir(&logs) {
        let mut paths = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            diagnostics.push(format!(
                "log {}:\n{}",
                path.display(),
                String::from_utf8_lossy(&std::fs::read(&path).unwrap_or_default())
            ));
        }
    }
    let ledger =
        std::fs::read_to_string(data.join("state/runtime-ledger.jsonl")).unwrap_or_default();
    diagnostics.push(format!(
        "ledger tail:\n{}",
        ledger
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    ));
    diagnostics.join("\n")
}

fn captured_command_output(
    stdout_path: &std::path::Path,
    stderr_path: &std::path::Path,
    status: ExitStatus,
) -> Output {
    let stdout = std::fs::read(stdout_path).unwrap_or_default();
    let stderr = std::fs::read(stderr_path).unwrap_or_default();
    let _ = std::fs::remove_file(stdout_path);
    let _ = std::fs::remove_file(stderr_path);
    Output {
        status,
        stdout,
        stderr,
    }
}

pub(super) fn native_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("native fixture ephemeral port reservation");
    listener
        .local_addr()
        .expect("native fixture local address")
        .port()
}
