#![cfg(unix)]

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "../workflow/recovery.rs"]
mod workflow_recovery;

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_CONCURRENT_FIXTURES: usize = 4;

struct FixtureLimiter {
    active: Mutex<usize>,
    ready: Condvar,
}

fn fixture_limiter() -> &'static FixtureLimiter {
    static LIMITER: OnceLock<FixtureLimiter> = OnceLock::new();
    LIMITER.get_or_init(|| FixtureLimiter {
        active: Mutex::new(0),
        ready: Condvar::new(),
    })
}

struct FixturePermit;

impl Drop for FixturePermit {
    fn drop(&mut self) {
        let limiter = fixture_limiter();
        let mut active = limiter.active.lock().unwrap();
        *active = active.saturating_sub(1);
        limiter.ready.notify_one();
    }
}

fn acquire_fixture_permit() -> FixturePermit {
    let limiter = fixture_limiter();
    let mut active = limiter.active.lock().unwrap();
    while *active >= MAX_CONCURRENT_FIXTURES {
        active = limiter.ready.wait(active).unwrap();
    }
    *active += 1;
    FixturePermit
}

fn fixture(name: &str) -> Fixture {
    let permit = acquire_fixture_permit();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("rpotato-{name}-{nonce}"));
    let project = root.join("project");
    let data = root.join("data");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("src/lib.rs"), "pub const VALUE: i32 = 1;\n").unwrap();
    let response = root.join("response.txt");
    fs::write(
        &response,
        "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none",
    )
    .unwrap();
    let calls = root.join("calls.txt");
    let backend = root.join("fake-llama-server");
    fs::write(
        &backend,
        format!(
            r#"#!/usr/bin/env python3
import argparse, json, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
p=argparse.ArgumentParser(add_help=False)
p.add_argument('--port', type=int, required=True)
p.add_argument('--host', default='127.0.0.1')
p.add_argument('--model')
p.add_argument('--ctx-size')
a,_=p.parse_known_args()
class H(BaseHTTPRequestHandler):
  def log_message(self, *args): pass
  def do_GET(self):
    self.send_response(200); self.end_headers(); self.wfile.write(b'{{"status":"ok"}}')
  def do_POST(self):
    n=int(self.headers.get('Content-Length','0')); request=json.loads(self.rfile.read(n))
    with open({calls:?}, 'a') as f: f.write('chat\n')
    with open({response:?}) as f: content=f.read()
    if request.get('stream'):
      prompt=request.get('messages',[{{}}])[-1].get('content','')
      if prompt == 'RPOTATO_STALL':
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
        try:
          while True:
            self.wfile.write(b': keepalive\n\n'); self.wfile.flush(); time.sleep(0.05)
        except (BrokenPipeError, ConnectionResetError):
          return
      if prompt == 'RPOTATO_UPSTREAM_ERROR':
        body=b'data: {{"error":{{"message":"RPOTATO_SECRET_UPSTREAM_DETAIL"}}}}\n\n'
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
        return
      if prompt == 'RPOTATO_HTTP_ERROR':
        self.wfile.write(b'HTTP/1.1 503 RPOTATO_SECRET_REASON_PHRASE\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
        return
      if prompt == 'RPOTATO_MIXED_LANGUAGE':
        parts=['정상 한국어 문장입니다. ', 'Forbidden English ', 'sentence.']
      else:
        parts=[content]
      events=[{{"choices":[{{"delta":{{"content":part}},"finish_reason":None}}]}} for part in parts]
      events[-1]['choices'][0]['finish_reason']='stop'
      events.append({{"choices":[],"usage":{{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}}}})
      body=(''.join('data: '+json.dumps(event)+'\n\n' for event in events)+'data: [DONE]\n\n').encode()
      content_type='text/event-stream'
    else:
      body=json.dumps({{"choices":[{{"message":{{"content":content}},"finish_reason":"stop"}}],"usage":{{"prompt_tokens":10,"completion_tokens":10,"total_tokens":20}}}}).encode()
      content_type='application/json'
    self.send_response(200); self.send_header('Content-Type',content_type); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
ThreadingHTTPServer((a.host,a.port),H).serve_forever()
"#,
            calls = calls.display().to_string(),
            response = response.display().to_string()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&backend).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&backend, permissions).unwrap();
    Fixture {
        root,
        project,
        data,
        backend,
        response,
        calls,
        port: AtomicU16::new(available_port()),
        _permit: permit,
    }
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    data: PathBuf,
    backend: PathBuf,
    response: PathBuf,
    calls: PathBuf,
    port: AtomicU16,
    _permit: FixturePermit,
}

impl Fixture {
    fn command_builder(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rpotato"));
        command
            .args(args)
            .env("RPOTATO_PROJECT_ROOT", &self.project)
            .env("RPOTATO_DATA_HOME", &self.data)
            .env("RPOTATO_BACKEND_LLAMA_CPP_PATH", &self.backend)
            .env(
                "RPOTATO_BACKEND_PORT",
                self.port.load(Ordering::Relaxed).to_string(),
            );
        command
    }

    fn command(&self, args: &[&str]) -> Output {
        let mut command = self.command_builder(args);
        let child = spawn_captured(&mut command).unwrap();
        wait_bounded(child, args)
    }

    fn start(&self) {
        fs::write(self.root.join("model.gguf"), b"fake model").unwrap();
        for attempt in 0..3 {
            let output = self.command(&[
                "backend",
                "start",
                "--model",
                self.root.join("model.gguf").to_str().unwrap(),
                "--ctx-size",
                "1024",
            ]);
            if output.status.success() {
                return;
            }
            let logs = fs::read_dir(self.data.join("logs"))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .filter_map(|entry| fs::read_to_string(entry.path()).ok())
                .collect::<Vec<_>>()
                .join("\n");
            if attempt < 2 && logs.contains("Address already in use") {
                self.port.store(available_port(), Ordering::Relaxed);
                continue;
            }
            panic!("{}\n{logs}", String::from_utf8_lossy(&output.stderr));
        }
        unreachable!("bounded backend start retry must return or panic");
    }

    fn stop(&self) {
        let mut command = self.command_builder(&["backend", "stop"]);
        if let Ok(child) = spawn_captured(&mut command) {
            let _ = wait_bounded_result(child, &["backend", "stop"]);
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn setup_failing_test_project(fixture: &Fixture) {
    fs::write(
        fixture.project.join("Cargo.toml"),
        "[package]\nname = \"rpotato-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir_all(fixture.project.join("tests")).unwrap();
    fs::write(
        fixture.project.join("tests/value.rs"),
        "use rpotato_fixture::VALUE;\n\n#[test]\nfn value_is_two() {\n    assert_eq!(VALUE, 2);\n}\n",
    )
    .unwrap();
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
        "rpotato-test-output-{}-{nonce}",
        std::process::id()
    ));
    let stdout_path = base.with_extension("stdout");
    let stderr_path = base.with_extension("stderr");
    let stdout = fs::File::create(&stdout_path)?;
    let stderr = fs::File::create(&stderr_path)?;
    command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    Ok(CapturedChild {
        child: command.spawn()?,
        stdout_path,
        stderr_path,
    })
}

fn wait_bounded(child: CapturedChild, label: &[&str]) -> Output {
    wait_bounded_result(child, label).unwrap_or_else(|message| panic!("{message}"))
}

fn wait_bounded_result(mut captured: CapturedChild, label: &[&str]) -> Result<Output, String> {
    let deadline = Instant::now() + SUBPROCESS_TIMEOUT;
    let status = loop {
        match captured.child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = captured.child.kill();
                let status = captured.child.wait().map_err(|err| err.to_string())?;
                let output = captured_output(&captured, status);
                return Err(format!(
                    "subprocess timeout after {:?}: {}\nstdout={}\nstderr={}",
                    SUBPROCESS_TIMEOUT,
                    label.join(" "),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Err(err) => {
                let _ = captured.child.kill();
                let _ = captured.child.wait();
                return Err(format!("subprocess wait 실패: {} ({err})", label.join(" ")));
            }
        }
    };
    Ok(captured_output(&captured, status))
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
    panic!(
        "path가 timeout 안에 생성되지 않았습니다: {}",
        path.display()
    );
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
    panic!(
        "line count가 timeout 안에 도달하지 않았습니다: {} expected {expected}",
        path.display()
    );
}

fn tree_contains(root: &Path, needle: &[u8]) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if tree_contains(&path, needle) {
                return true;
            }
        } else if fs::read(path)
            .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
            .unwrap_or(false)
        {
            return true;
        }
    }
    false
}

fn available_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("ephemeral backend port allocation")
        .local_addr()
        .expect("ephemeral backend port address")
        .port()
}

fn field(output: &str, key: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("- {key}: ")))
        .unwrap()
        .to_string()
}

fn command_token(output: &str, prefix: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string()
}

fn verification_token(output: &str) -> String {
    command_token(
        output,
        "- verification command approval: rpotato patch verify ",
    )
}

fn path_contains_bytes(path: &Path, needle: &[u8]) -> bool {
    if path.is_dir() {
        return fs::read_dir(path)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|entry| path_contains_bytes(&entry.path(), needle));
    }
    fs::read(path)
        .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
        .unwrap_or(false)
}

mod backend_runtime;
mod concurrency;
mod patch_safety;
mod workflow_journeys;
