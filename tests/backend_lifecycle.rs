use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(20);

#[test]
fn native_backend_cancel_and_stop_lifecycle() {
    let fixture = Fixture::new();
    let start = fixture.command(&[
        "backend",
        "start",
        "--model",
        fixture.model.to_str().unwrap(),
        "--ctx-size",
        "1024",
    ]);
    assert_success(&start, "backend start");

    let first_chat = fixture.spawn(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--stream",
        "--timeout-ms",
        "15000",
    ]);
    wait_for_path(&fixture.active_generation_path(), Duration::from_secs(5));
    wait_for_lines(&fixture.requests, 1, Duration::from_secs(5));
    let cancel = fixture.command(&["backend", "cancel"]);
    assert_success(&cancel, "backend cancel");
    assert!(
        text(&cancel.stdout).contains("terminal outcome: cancelled"),
        "stdout={}\nstderr={}",
        text(&cancel.stdout),
        text(&cancel.stderr)
    );
    let first_chat = wait_bounded(first_chat, "first backend chat");
    assert!(!first_chat.status.success());
    assert!(
        text(&first_chat.stderr).contains("취소됨"),
        "stdout={}\nstderr={}",
        text(&first_chat.stdout),
        text(&first_chat.stderr)
    );
    assert!(!fixture.active_generation_path().exists());

    let running = fixture.command(&["backend", "status"]);
    assert_success(&running, "backend status after cancel");
    assert!(text(&running.stdout).contains("status: running"));

    let second_chat = fixture.spawn(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--stream",
        "--timeout-ms",
        "15000",
    ]);
    wait_for_path(&fixture.active_generation_path(), Duration::from_secs(5));
    wait_for_lines(&fixture.requests, 2, Duration::from_secs(5));
    let stop = fixture.command(&["backend", "stop"]);
    assert_success(&stop, "backend stop");
    assert!(
        text(&stop.stdout).contains("generation outcome: cancelled"),
        "stdout={}\nstderr={}",
        text(&stop.stdout),
        text(&stop.stderr)
    );
    let second_chat = wait_bounded(second_chat, "second backend chat");
    assert!(!second_chat.status.success());
    assert!(
        text(&second_chat.stderr).contains("취소됨"),
        "stdout={}\nstderr={}",
        text(&second_chat.stdout),
        text(&second_chat.stderr)
    );

    let stopped = fixture.command(&["backend", "status"]);
    assert_success(&stopped, "backend status after stop");
    assert!(text(&stopped.stdout).contains("status: stopped"));
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    data: PathBuf,
    backend: PathBuf,
    model: PathBuf,
    requests: PathBuf,
    port: u16,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rpotato-native-lifecycle-{nonce}"));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(&project).unwrap();
        let backend = root.join(if cfg!(windows) {
            "fake-sidecar.exe"
        } else {
            "fake-sidecar"
        });
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_sidecar.rs");
        let compile = Command::new("rustc")
            .arg("--edition=2021")
            .arg(source)
            .arg("-o")
            .arg(&backend)
            .output()
            .unwrap();
        assert_success(&compile, "compile fake sidecar");
        let model = root.join("model.gguf");
        fs::write(&model, b"fake model").unwrap();
        let requests = root.join("requests.txt");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        Self {
            root,
            project,
            data,
            backend,
            model,
            requests,
            port,
        }
    }

    fn command_builder(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rpotato"));
        command
            .args(args)
            .env("RPOTATO_PROJECT_ROOT", &self.project)
            .env("RPOTATO_DATA_HOME", &self.data)
            .env("RPOTATO_BACKEND_LLAMA_CPP_PATH", &self.backend)
            .env("RPOTATO_BACKEND_PORT", self.port.to_string())
            .env("RPOTATO_FAKE_REQUEST_MARKER", &self.requests);
        command
    }

    fn command(&self, args: &[&str]) -> Output {
        wait_bounded(self.spawn(args), &args.join(" "))
    }

    fn spawn(&self, args: &[&str]) -> CapturedChild {
        let mut command = self.command_builder(args);
        spawn_captured(&mut command).unwrap()
    }

    fn active_generation_path(&self) -> PathBuf {
        self.data.join("state/backend-active-generation.txt")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.command(&["backend", "stop"]);
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct CapturedChild {
    child: Child,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
}

fn spawn_captured(command: &mut Command) -> std::io::Result<CapturedChild> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "rpotato-native-output-{}-{nonce}",
        std::process::id()
    ));
    let stdout_path = base.with_extension("stdout");
    let stderr_path = base.with_extension("stderr");
    command
        .stdout(Stdio::from(fs::File::create(&stdout_path)?))
        .stderr(Stdio::from(fs::File::create(&stderr_path)?));
    Ok(CapturedChild {
        child: command.spawn()?,
        stdout_path,
        stderr_path,
    })
}

fn wait_bounded(mut captured: CapturedChild, label: &str) -> Output {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    let status = loop {
        match captured.child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = captured.child.kill();
                let status = captured.child.wait().unwrap();
                let output = captured_output(&captured, status);
                panic!(
                    "{label} timeout\nstdout={}\nstderr={}",
                    text(&output.stdout),
                    text(&output.stderr)
                );
            }
            Err(err) => panic!("{label} wait 실패: {err}"),
        }
    };
    captured_output(&captured, status)
}

fn captured_output(captured: &CapturedChild, status: ExitStatus) -> Output {
    let stdout = fs::read(&captured.stdout_path).unwrap_or_default();
    let stderr = fs::read(&captured.stderr_path).unwrap_or_default();
    let _ = fs::remove_file(&captured.stdout_path);
    let _ = fs::remove_file(&captured.stderr_path);
    Output {
        status,
        stdout,
        stderr,
    }
}

fn wait_for_path(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("path timeout: {}", path.display());
}

fn wait_for_lines(path: &Path, expected: usize, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let lines = fs::read_to_string(path)
            .map(|text| text.lines().count())
            .unwrap_or(0);
        if lines >= expected {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("line count timeout: {} expected {expected}", path.display());
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} 실패\nstdout={}\nstderr={}",
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
