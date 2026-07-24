#![cfg(unix)]

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "../workflow/recovery.rs"]
mod workflow_recovery;

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(20);

fn fixture(name: &str) -> Fixture {
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

#[test]
fn fixture_retries_backend_start_after_ephemeral_port_collision() {
    let fixture = fixture("backend-port-retry");
    let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let occupied_port = occupied.local_addr().unwrap().port();
    fixture.port.store(occupied_port, Ordering::Relaxed);

    fixture.start();

    assert_ne!(fixture.port.load(Ordering::Relaxed), occupied_port);
}

#[test]
fn happy_path_is_restart_safe_and_reports_korean() {
    let fixture = fixture("happy-subprocess");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let run_out = String::from_utf8(run.stdout).unwrap();
    assert!(!run_out.contains("MODEL ACTION"));
    assert!(!run_out.contains("- response:"));
    assert!(run_out.contains("raw response는 표시하지 않음"));
    let proposal = field(&run_out, "proposal id");
    let token = run_out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let resume = fixture.command(&["state", "resume"]);
    assert!(resume.status.success());
    let resume_out = String::from_utf8_lossy(&resume.stdout);
    assert!(resume_out.contains("backend 호출: 없음"));
    assert!(resume_out.contains("token 재표시: 불가"));
    assert!(!resume_out.contains(&token));
    let tui = fixture.command(&["tui", "diff", &proposal]);
    assert!(tui.status.success());
    assert!(!String::from_utf8_lossy(&tui.stdout).contains(&token));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );

    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(
        approve.status.success(),
        "{}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    assert!(approve_report.starts_with("patch approve\n- status: applied-awaiting-verification"));
    assert!(approve_report.contains("verification approval: required"));
    assert!(approve_report.contains("verification command는 아직 실행하지 않았습니다"));
    assert!(!approve_report.contains("패치 작업 완료"));
    assert!(!approve_report.contains("MODEL ACTION"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let verify_token = verification_token(&approve_report);

    let resumed = fixture.command(&["state", "resume"]);
    assert!(resumed.status.success());
    let resumed_out = String::from_utf8_lossy(&resumed.stdout);
    assert!(resumed_out.contains("verification 승인 대기"));
    assert!(resumed_out.contains("verification 실행: 없음"));
    assert!(!resumed_out.contains(&verify_token));

    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let report = String::from_utf8(verify.stdout).unwrap();
    assert!(report.starts_with("패치 작업 완료\n- 결과: 성공"));
    assert!(report.contains("stop gate: 통과"));
    assert!(!report.contains("MODEL ACTION"));

    let ledger_path = fixture.data.join("state/runtime-ledger.jsonl");
    let event_count = fs::read_to_string(&ledger_path).unwrap().lines().count();
    let repeated = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        repeated.status.success(),
        "status={:?}\nstderr={}\nstdout={}",
        repeated.status.code(),
        String::from_utf8_lossy(&repeated.stderr),
        String::from_utf8_lossy(&repeated.stdout)
    );
    assert_eq!(
        fs::read_to_string(&ledger_path).unwrap().lines().count(),
        event_count,
        "complete resume must not duplicate ledger events"
    );
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );
}

#[test]
fn explicit_skill_run_persists_lifecycle_state_and_sqlite_projection() {
    let fixture = fixture("explicit-skill-lifecycle");
    fixture.start();

    let run = fixture.command(&[
        "skill",
        "run",
        "small-patch",
        "src/lib.rs의 값을 2로 고쳐줘",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    assert!(report.contains("- invocation: explicit-skill"));
    assert!(report.contains("- selected skill: small-patch"));
    let workflow_id = field(&report, "workflow id");
    let proposal = field(&report, "proposal id");
    let token = command_token(&report, "- approval command: rpotato patch approve ");

    let pending = latest_workflow_snapshot(&fixture, &workflow_id);
    assert!(pending.contains("\"active_skill_id\": \"small-patch\""));
    assert!(pending.contains("\"skill_invocation\": \"explicit\""));
    assert!(pending.contains("\"skill_state\": \"awaiting-approval\""));
    assert!(pending.contains("pre_model_request"));
    assert!(pending.contains("diff_review"));

    let connection =
        rusqlite::Connection::open(fixture.data.join("state/observability.sqlite")).unwrap();
    let projected_skill: String = connection
        .query_row(
            "SELECT active_skill_id FROM workflows WHERE workflow_id = ?1",
            [&workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(projected_skill, "small-patch");

    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(approve.status.success());
    let verify_token = verification_token(&String::from_utf8(approve.stdout).unwrap());
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );

    let complete = latest_workflow_snapshot(&fixture, &workflow_id);
    assert!(complete.contains("\"skill_state\": \"complete\""));
    assert!(complete.contains("diff_review,targeted_verification"));
    assert!(complete.contains("patch_applied,verification_passed,korean_report_passed"));
    for hook in [
        "session_start",
        "user_request_received",
        "pre_context_pack",
        "post_context_pack",
        "pre_model_request",
        "post_model_response",
        "pre_action_parse",
        "post_action_parse",
        "pre_tool_call",
        "post_tool_result",
        "pre_patch_apply",
        "post_patch_apply",
        "pre_command_run",
        "post_command_run",
        "pre_final_report",
        "stop_gate",
        "session_end",
    ] {
        assert!(complete.contains(hook), "missing persisted hook: {hook}");
    }
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("\"event_type\":\"hook.dispatched\""));
    assert!(ledger.contains("hook=session_start"));
    assert!(ledger.contains("hook=stop_gate"));
}

#[test]
fn explicit_skill_missing_context_fails_before_model_call() {
    let fixture = fixture("explicit-skill-missing-context");
    fixture.start();

    let run = fixture.command(&["skill", "run", "fix-test", "실패한 테스트를 고쳐줘"]);

    assert_eq!(run.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("skill requirement 차단"),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!fixture.calls.exists());
    let state = fixture.command(&["state"]);
    assert!(state.status.success());
    assert!(String::from_utf8_lossy(&state.stdout).contains("active workflow: 없음"));
}

#[test]
fn fix_test_records_real_failure_before_patch_and_pass_after_patch() {
    let fixture = fixture("fix-test-real-evidence");
    setup_failing_test_project(&fixture);
    fs::write(
        &fixture.response,
        "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=cargo test; next_gate=diff-before-write; side_effects=none",
    )
    .unwrap();
    fixture.start();

    let run = fixture.command(&[
        "skill",
        "run",
        "fix-test",
        "src/lib.rs 테스트 결과: test result: FAILED, VALUE는 2여야 합니다.",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    let workflow_id = field(&report, "workflow id");
    let proposal = field(&report, "proposal id");
    let token = command_token(&report, "- approval command: rpotato patch approve ");
    let pending = latest_workflow_snapshot(&fixture, &workflow_id);
    assert!(pending.contains("failing_test_before"));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("\"event_type\":\"skill.test_failure.observed\""));
    assert!(ledger.contains(&format!("workflow_id={workflow_id}")));

    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(
        approve.status.success(),
        "{}",
        String::from_utf8_lossy(&approve.stderr)
    );
    let verify_token = verification_token(&String::from_utf8(approve.stdout).unwrap());
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );

    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let complete = latest_workflow_snapshot(&fixture, &workflow_id);
    assert!(complete.contains("\"skill_state\": \"complete\""));
    assert!(complete.contains("failing_test_before,passing_test_after"));
}

#[test]
fn fix_test_rejects_non_test_verification_without_leaving_active_workflow() {
    let fixture = fixture("fix-test-non-test-verification");
    setup_failing_test_project(&fixture);
    fixture.start();

    let run = fixture.command(&[
        "skill",
        "run",
        "fix-test",
        "src/lib.rs 테스트 결과: test result: FAILED, VALUE는 2여야 합니다.",
    ]);

    assert_eq!(run.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&run.stderr).contains("cargo test"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );
    let state = fixture.command(&["state"]);
    assert!(state.status.success());
    assert!(String::from_utf8_lossy(&state.stdout).contains("active workflow: 없음"));
}

#[test]
fn read_only_action_without_visible_answer_fails_closed() {
    let fixture = fixture("read-only-empty-answer");
    fs::write(
        &fixture.response,
        "MODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();

    let run = fixture.command(&["skill", "run", "repo-map", "저장소 구조를 분석해줘"]);

    assert_eq!(run.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&run.stderr).contains("답변이 비어 있습니다"));
    let state = fixture.command(&["state"]);
    assert!(state.status.success());
    assert!(String::from_utf8_lossy(&state.stdout).contains("active workflow: 없음"));
}

fn latest_workflow_snapshot(fixture: &Fixture, workflow_id: &str) -> String {
    let snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{workflow_id}.snapshots"));
    let latest = fs::read_dir(snapshots)
        .unwrap()
        .filter_map(Result::ok)
        .max_by_key(|entry| entry.file_name())
        .unwrap();
    fs::read_to_string(latest.path()).unwrap()
}

#[test]
fn durable_transcript_rebuilds_after_db_loss_and_continue_is_idempotent() {
    let fixture = fixture("durable-conversation-resume");
    fixture.start();

    let run = fixture.command(&["run", "src/lib.rs의 값을 2로 고쳐줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let list = fixture.command(&["session", "list"]);
    assert!(list.status.success());
    let session_id = String::from_utf8(list.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("- current session: "))
        .unwrap()
        .to_string();

    let transcript = fixture.command(&["tui", "transcript", &session_id]);
    assert!(transcript.status.success());
    let transcript_report = String::from_utf8(transcript.stdout).unwrap();
    assert!(transcript_report.contains("[durable conversation]"));
    assert!(transcript_report.contains("user |"));
    assert!(transcript_report.contains("tool |"));
    assert!(transcript_report.contains("model |"));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );

    let db = fixture.data.join("state/observability.sqlite");
    let _ = fs::remove_file(&db);
    let _ = fs::remove_file(db.with_extension("sqlite-wal"));
    let _ = fs::remove_file(db.with_extension("sqlite-shm"));

    for args in [vec!["continue"], vec!["resume", session_id.as_str()]] {
        let resumed = fixture.command(&args);
        assert!(
            resumed.status.success(),
            "{}",
            String::from_utf8_lossy(&resumed.stderr)
        );
        let report = String::from_utf8(resumed.stdout).unwrap();
        assert!(
            report.contains("reconstructed context: context limit=1024 transcript turns=3"),
            "{report}"
        );
        assert!(report.contains("backend 호출: 없음"));
        assert_eq!(
            fs::read_to_string(&fixture.calls).unwrap().lines().count(),
            1
        );
    }

    let status = fixture.command(&["state"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(String::from_utf8(status.stdout)
        .unwrap()
        .contains("transcript records: 3"));

    let project_transcripts = fixture.data.join("state/transcripts");
    let project_dir = fs::read_dir(project_transcripts)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let session_dir = project_dir.join(&session_id);
    let artifact = fs::read_dir(session_dir)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .expect("canonical transcript JSON artifact");
    fs::write(artifact, "{}\n").unwrap();

    let blocked = fixture.command(&["continue"]);
    assert_eq!(blocked.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );
}

#[test]
fn patch_transcript_excludes_source_fragments_from_durable_surfaces() {
    const SECRET: &str = "RPOTATO_SECRET_SOURCE_FRAGMENT";
    let fixture = fixture("transcript-source-redaction");
    fs::write(
        fixture.project.join("src/lib.rs"),
        format!("pub const VALUE: &str = \"{SECRET}\";\n"),
    )
    .unwrap();
    let find_hex = SECRET
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    fs::write(
        &fixture.response,
        format!(
            "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex={find_hex}; replace_hex=7265646163746564; verification=pwd; next_gate=diff-before-write; side_effects=none"
        ),
    )
    .unwrap();
    fixture.start();

    let run = fixture.command(&["run", "상수 값을 안전한 값으로 바꿔줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let list = fixture.command(&["session", "list"]);
    let session_id = String::from_utf8(list.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("- current session: "))
        .unwrap()
        .to_string();
    let tui = fixture.command(&["tui", "transcript", &session_id]);
    assert!(tui.status.success());
    assert!(!String::from_utf8_lossy(&tui.stdout).contains(SECRET));

    for path in [
        fixture.data.join("state/transcripts"),
        fixture.data.join("state/runtime-ledger.jsonl"),
        fixture.data.join("state/observability.sqlite"),
    ] {
        assert!(
            !path_contains_bytes(&path, SECRET.as_bytes()),
            "secret leaked into {}",
            path.display()
        );
    }
}

#[test]
fn read_only_run_completes_without_patch_gate() {
    let fixture = fixture("read-only-subprocess");
    fs::write(
        &fixture.response,
        "src/lib.rs 구조를 확인했으며 파일 변경은 필요하지 않습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();

    let run = fixture.command(&["run", "저장소 구조를 분석해줘"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    assert!(report.starts_with("run 결과\n- 상태: 완료"));
    assert!(report.contains("- action kind: inspect-sources"));
    assert!(report.contains("- side effect: 없음"));
    assert!(report.contains("src/lib.rs 구조를 확인했으며 파일 변경은 필요하지 않습니다."));
    assert!(!report.contains("MODEL ACTION"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let workflow_id = field(&report, "workflow id");
    let snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{workflow_id}.snapshots"));
    let latest = fs::read_dir(snapshots)
        .unwrap()
        .filter_map(Result::ok)
        .max_by_key(|entry| entry.file_name())
        .unwrap();
    let stored = fs::read_to_string(latest.path()).unwrap();
    assert!(stored.contains("\"workflow_kind\": \"agent-run\""));
    assert!(stored.contains("\"action_kind\": \"inspect-sources\""));
    assert!(stored.contains("\"phase\": \"complete\""));

    let status = fixture.command(&["state"]);
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(String::from_utf8_lossy(&status.stdout).contains("active workflow: 없음"));
}

#[test]
fn imported_codex_skill_runs_through_read_only_runtime_boundaries() {
    let fixture = fixture("imported-codex-skill");
    let plugin = fixture.root.join("safe-plugin");
    fs::create_dir_all(plugin.join(".codex-plugin")).unwrap();
    fs::create_dir_all(plugin.join("skills/hello")).unwrap();
    fs::write(
        plugin.join(".codex-plugin/plugin.json"),
        r#"{"name":"safe-plugin","version":"1.0.0","description":"safe"}"#,
    )
    .unwrap();
    fs::write(
        plugin.join("skills/hello/SKILL.md"),
        "---\nname: hello\ndescription: 저장소를 읽기 전용으로 설명한다.\n---\n근거 파일을 확인하고 한국어로 설명하세요.\n",
    )
    .unwrap();
    fs::write(
        &fixture.response,
        "src/lib.rs를 읽기 전용으로 확인했으며 파일은 변경하지 않았습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();

    for args in [
        vec![
            "plugin",
            "import",
            "--from",
            "codex",
            plugin.to_str().unwrap(),
        ],
        vec!["plugin", "validate", "imported.codex.safe-plugin"],
        vec!["plugin", "enable", "imported.codex.safe-plugin"],
    ] {
        let output = fixture.command(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let run = fixture.command(&[
        "skill",
        "run",
        "imported.codex.safe-plugin.hello",
        "현재 저장소를 설명해줘",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    assert!(report.contains("- plugin boundary: instruction-only/read-only"));
    assert!(report.contains("- plugin source: skills/hello/SKILL.md@"));
    assert!(report.contains("src/lib.rs를 읽기 전용으로 확인"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let workflow_id = field(&report, "workflow id");
    let snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{workflow_id}.snapshots"));
    let latest = fs::read_dir(snapshots)
        .unwrap()
        .filter_map(Result::ok)
        .max_by_key(|entry| entry.file_name())
        .unwrap();
    let stored = fs::read_to_string(latest.path()).unwrap();
    assert!(stored.contains("\"workflow_kind\": \"plugin-capability\""));
    assert!(stored.contains("\"source_path\": \"skills/hello/SKILL.md\""));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("plugin.capability.admitted"));
    assert!(ledger.contains("plugin.capability.completed"));
}

#[test]
fn imported_claude_command_runs_through_read_only_runtime_boundaries() {
    let fixture = fixture("imported-claude-command");
    let plugin = fixture.root.join("safe-claude-plugin");
    fs::create_dir_all(plugin.join(".claude-plugin")).unwrap();
    fs::create_dir_all(plugin.join("commands")).unwrap();
    fs::write(
        plugin.join(".claude-plugin/plugin.json"),
        r#"{"name":"safe-claude-plugin","version":"1.0.0","description":"safe"}"#,
    )
    .unwrap();
    fs::write(
        plugin.join("commands/explain.md"),
        "---\ndescription: 저장소를 읽기 전용으로 설명한다.\n---\n근거 파일을 확인하고 한국어로 설명하세요.\n",
    )
    .unwrap();
    fs::write(
        &fixture.response,
        "src/lib.rs를 읽기 전용으로 확인했으며 파일은 변경하지 않았습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
    )
    .unwrap();
    fixture.start();

    for args in [
        vec![
            "plugin",
            "import",
            "--from",
            "claude-code",
            plugin.to_str().unwrap(),
        ],
        vec![
            "plugin",
            "validate",
            "imported.claude-code.safe-claude-plugin",
        ],
        vec![
            "plugin",
            "enable",
            "imported.claude-code.safe-claude-plugin",
        ],
    ] {
        let output = fixture.command(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let run = fixture.command(&[
        "skill",
        "run",
        "imported.claude-code.safe-claude-plugin.explain",
        "현재 저장소를 설명해줘",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let report = String::from_utf8(run.stdout).unwrap();
    assert!(report.contains("- plugin boundary: instruction-only/read-only"));
    assert!(report.contains("- plugin source: commands/explain.md@"));
    assert!(report.contains("src/lib.rs를 읽기 전용으로 확인"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let workflow_id = field(&report, "workflow id");
    let snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{workflow_id}.snapshots"));
    let latest = fs::read_dir(snapshots)
        .unwrap()
        .filter_map(Result::ok)
        .max_by_key(|entry| entry.file_name())
        .unwrap();
    let stored = fs::read_to_string(latest.path()).unwrap();
    assert!(stored.contains("\"workflow_kind\": \"plugin-capability\""));
    assert!(stored.contains("\"source_path\": \"commands/explain.md\""));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("plugin.capability.admitted"));
    assert!(ledger.contains("plugin.capability.completed"));
}

#[test]
fn imported_codex_skill_completion_recovery_is_idempotent() {
    for (fault, expected_before) in [("before-event", 0), ("before-pointer-clear", 1)] {
        let fixture = fixture(&format!("imported-codex-recovery-{fault}"));
        let plugin = fixture.root.join("safe-plugin");
        fs::create_dir_all(plugin.join(".codex-plugin")).unwrap();
        fs::create_dir_all(plugin.join("skills/hello")).unwrap();
        fs::write(
            plugin.join(".codex-plugin/plugin.json"),
            r#"{"name":"safe-plugin","version":"1.0.0","description":"safe"}"#,
        )
        .unwrap();
        fs::write(
            plugin.join("skills/hello/SKILL.md"),
            "---\nname: hello\ndescription: 저장소를 읽기 전용으로 설명한다.\n---\n근거 파일을 확인하고 한국어로 설명하세요.\n",
        )
        .unwrap();
        fs::write(
            &fixture.response,
            "src/lib.rs를 읽기 전용으로 확인했으며 파일은 변경하지 않았습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
        )
        .unwrap();
        fixture.start();

        for args in [
            vec![
                "plugin",
                "import",
                "--from",
                "codex",
                plugin.to_str().unwrap(),
            ],
            vec!["plugin", "validate", "imported.codex.safe-plugin"],
            vec!["plugin", "enable", "imported.codex.safe-plugin"],
        ] {
            let output = fixture.command(&args);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let args = [
            "skill",
            "run",
            "imported.codex.safe-plugin.hello",
            "현재 저장소를 설명해줘",
        ];
        let mut command = fixture.command_builder(&args);
        command.env("RPOTATO_TEST_PLUGIN_COMPLETION_FAULT", fault);
        let child = spawn_captured(&mut command).unwrap();
        let interrupted = wait_bounded(child, &args);
        assert!(!interrupted.status.success());
        let interrupted_error = String::from_utf8_lossy(&interrupted.stderr);
        assert!(!interrupted_error.is_empty(), "missing error for {fault}");

        let ledger_path = fixture.data.join("state/runtime-ledger.jsonl");
        let before = fs::read_to_string(&ledger_path).unwrap();
        assert_eq!(
            before.matches("plugin.capability.completed").count(),
            expected_before
        );

        let resume = fixture.command(&["state", "resume"]);
        assert!(
            resume.status.success(),
            "{}",
            String::from_utf8_lossy(&resume.stderr)
        );
        assert!(String::from_utf8_lossy(&resume.stdout).contains("plugin capability 복구 완료"));
        let after = fs::read_to_string(&ledger_path).unwrap();
        assert_eq!(after.matches("plugin.capability.completed").count(), 1);

        let status = fixture.command(&["state"]);
        assert!(status.status.success());
        assert!(String::from_utf8_lossy(&status.stdout).contains("active workflow: 없음"));
    }
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

#[test]
fn complete_resume_revalidates_deleted_evidence() {
    let fixture = fixture("complete-evidence-delete");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&out, "proposal id");
    let token = out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();
    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(approve.status.success());
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    let verify_token = verification_token(&approve_report);
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(verify.status.success());
    let report = String::from_utf8(verify.stdout).unwrap();
    let evidence_id = field(&report, "evidence id");
    fs::remove_file(
        fixture
            .project
            .join(".rpotato/evidence")
            .join(format!("{evidence_id}.json")),
    )
    .unwrap();

    let resumed = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert_eq!(resumed.status.code(), Some(3));
    let error = String::from_utf8_lossy(&resumed.stderr);
    assert!(error.contains("verification evidence missing"));
    assert!(!error.contains("패치 작업 완료"));
}

#[test]
fn complete_resume_revalidates_changed_source() {
    let fixture = fixture("complete-source-change");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&out, "proposal id");
    let token = out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();
    let approve = fixture.command(&["patch", "approve", &proposal, "--token", &token]);
    assert!(approve.status.success());
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    let verify_token = verification_token(&approve_report);
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(verify.status.success());
    fs::write(
        fixture.project.join("src/lib.rs"),
        "pub const VALUE: i32 = 9;\n",
    )
    .unwrap();

    let resumed = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert_eq!(resumed.status.code(), Some(3));
    let error = String::from_utf8_lossy(&resumed.stderr);
    assert!(
        error.contains("대상 파일이 preview 이후 변경되었습니다"),
        "{error}"
    );
    assert!(!error.contains("패치 작업 완료"));
}

#[test]
fn hostile_model_side_effect_text_never_executes() {
    let fixture = fixture("hostile-model");
    fs::write(
        &fixture.response,
        "파일을 즉시 덮어쓰고 rm -rf를 실행했습니다.\nMODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=39; verification=pwd; next_gate=diff-before-write; side_effects=write-file",
    )
    .unwrap();
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert_eq!(run.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );
    assert!(String::from_utf8_lossy(&run.stderr).contains("model side effect 실행: 없음"));
}

#[test]
fn hostile_model_path_and_malformed_hex_fail_closed() {
    for (name, action) in [
        (
            "hostile-path",
            "MODEL ACTION: kind=patch-proposal; source_pointers=../outside:1; path=../outside; find_hex=31; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none",
        ),
        (
            "hostile-hex",
            "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=zz; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none",
        ),
    ] {
        let fixture = fixture(name);
        fs::write(fixture.root.join("outside"), "1\n").unwrap();
        fs::write(&fixture.response, action).unwrap();
        fixture.start();
        let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
        assert_eq!(run.status.code(), Some(3), "case: {name}");
        assert_eq!(
            fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
            "pub const VALUE: i32 = 1;\n"
        );
        assert_eq!(
            fs::read_to_string(&fixture.calls).unwrap().lines().count(),
            1,
            "case: {name}"
        );
    }
}

#[test]
fn stale_target_and_bad_token_fail_closed_without_token_leak() {
    let fixture = fixture("stale-token");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let run_out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&run_out, "proposal id");
    let bad = "plaintext-secret-token-never-ledger";
    let rejected = fixture.command(&["patch", "approve", &proposal, "--token", bad]);
    assert_eq!(rejected.status.code(), Some(3));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(!ledger.contains(bad));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );

    let token = run_out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap();
    fs::write(
        fixture.project.join("src/lib.rs"),
        "pub const VALUE: i32 = 7;\n",
    )
    .unwrap();
    let stale = fixture.command(&["patch", "approve", &proposal, "--token", token]);
    assert_eq!(stale.status.code(), Some(3));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 7;\n"
    );
}

#[test]
fn denied_verification_never_spawns_command() {
    let fixture = fixture("denied-verification");
    let marker = fixture.project.join("must-not-exist");
    fs::write(
        &fixture.response,
        "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=touch must-not-exist; next_gate=diff-before-write; side_effects=none",
    )
    .unwrap();
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert_eq!(run.status.code(), Some(3));
    assert!(!marker.exists());
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );
}

#[test]
fn verification_failure_restores_original_and_blocks_success() {
    let fixture = fixture("verification-rollback");
    fs::write(
        &fixture.response,
        "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=cargo test; next_gate=diff-before-write; side_effects=none",
    )
    .unwrap();
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&out, "proposal id");
    let token = out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap();
    let approve = fixture.command(&["patch", "approve", &proposal, "--token", token]);
    assert!(approve.status.success());
    let approve_report = String::from_utf8(approve.stdout).unwrap();
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let verify_token = verification_token(&approve_report);
    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert_eq!(verify.status.code(), Some(3));
    let error = String::from_utf8_lossy(&verify.stderr);
    assert!(
        error.contains("verification-failed-rolled-back"),
        "status={:?}\nstderr={}\nstdout={}",
        verify.status.code(),
        error,
        String::from_utf8_lossy(&verify.stdout)
    );
    assert!(!error.contains("패치 작업 완료"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 1;\n"
    );
}

#[test]
fn corrupt_workflow_blocks_resume_without_backend_reentry() {
    let fixture = fixture("corrupt-workflow");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let workflow = field(&out, "workflow id");
    fs::write(
        fixture
            .project
            .join(".rpotato/workflows")
            .join(format!("{workflow}.json")),
        "{corrupt",
    )
    .unwrap();
    let resume = fixture.command(&["state", "resume"]);
    assert_eq!(resume.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&resume.stderr).contains("fail-closed"));
    assert_eq!(
        fs::read_to_string(&fixture.calls).unwrap().lines().count(),
        1
    );
}

#[test]
fn backend_generation_cancel_keeps_sidecar_and_cleans_active_state() {
    let fixture = fixture("backend-generation-cancel");
    fixture.start();
    let mut command = fixture.command_builder(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--stream",
        "--timeout-ms",
        "5000",
    ]);
    let chat = spawn_captured(&mut command).unwrap();
    let active_record = fixture.data.join("state/backend-active-generation.txt");
    wait_for_path(&active_record, Duration::from_secs(2));

    let cancel = fixture.command(&["backend", "cancel"]);
    assert!(
        cancel.status.success(),
        "{}",
        String::from_utf8_lossy(&cancel.stderr)
    );
    let chat = wait_bounded(chat, &["backend", "chat", "--stream"]);
    assert!(!chat.status.success());
    assert!(
        String::from_utf8_lossy(&chat.stderr).contains("취소됨"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&chat.stdout),
        String::from_utf8_lossy(&chat.stderr)
    );

    let status = fixture.command(&["backend", "status"]);
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("status: running"));
    assert!(!active_record.exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.lock")
        .exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.cancel")
        .exists());
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("backend.generation.cancelled"));
    assert!(ledger.contains("backend.resource.sampled"));
}

#[test]
fn backend_generation_timeout_records_terminal_evidence_and_cleans_state() {
    let fixture = fixture("backend-generation-timeout");
    fixture.start();

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--timeout-ms",
        "150",
    ]);
    assert!(!chat.status.success());
    assert!(
        String::from_utf8_lossy(&chat.stderr).contains("시간 초과"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&chat.stdout),
        String::from_utf8_lossy(&chat.stderr)
    );

    let status = fixture.command(&["backend", "status"]);
    assert!(status.status.success());
    assert!(String::from_utf8_lossy(&status.stdout).contains("status: running"));
    assert!(!fixture
        .data
        .join("state/backend-active-generation.txt")
        .exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.lock")
        .exists());
    assert!(!fixture
        .data
        .join("state/backend-active-generation.cancel")
        .exists());
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("backend.generation.timeout"));
    assert!(ledger.contains("backend.resource.sampled"));
}

#[test]
fn streaming_guard_keeps_a_nonempty_fallback_visible_without_failed_ledger() {
    let fixture = fixture("backend-stream-language-guard");
    let forbidden = "This model response must never be emitted.";
    fs::write(&fixture.response, forbidden).unwrap();
    fixture.start();

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "언어 경계를 검증해줘",
        "--stream",
    ]);

    assert!(chat.status.success());
    assert!(String::from_utf8_lossy(&chat.stdout).contains(forbidden));
    assert!(!String::from_utf8_lossy(&chat.stderr).contains(forbidden));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(!ledger.contains("backend.generation.failed"));
    assert!(ledger.contains("backend.chat.completed"));
    assert!(!ledger.contains(forbidden));
}

#[test]
fn upstream_stream_error_detail_is_redacted_from_output_and_persistent_state() {
    let fixture = fixture("backend-stream-error-redaction");
    fixture.start();
    let secret = b"RPOTATO_SECRET_UPSTREAM_DETAIL";

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_UPSTREAM_ERROR",
        "--stream",
    ]);

    assert_eq!(chat.status.code(), Some(3));
    assert!(!chat
        .stdout
        .windows(secret.len())
        .any(|window| window == secret));
    assert!(!chat
        .stderr
        .windows(secret.len())
        .any(|window| window == secret));
    assert!(!tree_contains(&fixture.data, secret));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("error_detail=redacted"));
}

#[test]
fn upstream_http_reason_phrase_is_redacted_from_output_and_persistent_state() {
    let fixture = fixture("backend-http-error-redaction");
    fixture.start();
    let secret = b"RPOTATO_SECRET_REASON_PHRASE";

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_HTTP_ERROR",
        "--stream",
    ]);

    assert_eq!(chat.status.code(), Some(3));
    assert!(!chat
        .stdout
        .windows(secret.len())
        .any(|window| window == secret));
    assert!(!chat
        .stderr
        .windows(secret.len())
        .any(|window| window == secret));
    assert!(String::from_utf8_lossy(&chat.stderr).contains("backend request 실패"));
    assert!(!tree_contains(&fixture.data, secret));
}

#[test]
fn streaming_guard_projects_mixed_output_without_hiding_the_answer() {
    let fixture = fixture("backend-stream-mixed-language-guard");
    fixture.start();
    let forbidden = "Forbidden English sentence.";

    let chat = fixture.command(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_MIXED_LANGUAGE",
        "--stream",
    ]);

    assert!(chat.status.success());
    let stdout = String::from_utf8_lossy(&chat.stdout);
    assert!(stdout.contains("정상 한국어 문장입니다."));
    assert!(!stdout.contains(forbidden));
    assert!(!String::from_utf8_lossy(&chat.stderr).contains(forbidden));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(!ledger.contains("backend.generation.failed"));
    assert!(ledger.contains("backend.chat.completed"));
    assert!(!ledger.contains(forbidden));
}

#[test]
fn backend_stop_acknowledges_generation_cancellation_before_sidecar_shutdown() {
    let fixture = fixture("backend-stop-active-generation");
    fixture.start();
    let mut command = fixture.command_builder(&[
        "backend",
        "chat",
        "--prompt",
        "RPOTATO_STALL",
        "--stream",
        "--timeout-ms",
        "15000",
    ]);
    let chat = spawn_captured(&mut command).unwrap();
    wait_for_path(
        &fixture.data.join("state/backend-active-generation.txt"),
        Duration::from_secs(5),
    );
    wait_for_lines(&fixture.calls, 1, Duration::from_secs(5));

    let stop = fixture.command(&["backend", "stop"]);
    let chat = wait_bounded(chat, &["backend", "chat", "--stream"]);

    assert!(stop.status.success());
    assert!(
        String::from_utf8_lossy(&stop.stdout).contains("generation outcome: cancelled"),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&stop.stdout),
        String::from_utf8_lossy(&stop.stderr)
    );
    assert!(!chat.status.success());
    assert!(String::from_utf8_lossy(&chat.stderr).contains("취소됨"));
    for name in [
        "backend-active-generation.txt",
        "backend-active-generation.lock",
        "backend-active-generation.cancel",
    ] {
        assert!(!fixture.data.join("state").join(name).exists(), "{name}");
    }
    let status = fixture.command(&["backend", "status"]);
    assert!(String::from_utf8_lossy(&status.stdout).contains("status: stopped"));
}

#[test]
fn token_rotate_recovers_lost_delivery_and_invalidates_old_token_across_processes() {
    let fixture = fixture("token-rotate-subprocess");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&out, "proposal id");
    let old_token = out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();

    let rotate = fixture.command(&["patch", "token-rotate", &proposal]);
    assert!(rotate.status.success());
    let rotate_out = String::from_utf8(rotate.stdout).unwrap();
    let new_token = field(&rotate_out, "새 approval token");
    let old = fixture.command(&[
        "patch",
        "approve",
        &proposal,
        "--token",
        &old_token,
        "--dry-run",
    ]);
    let new = fixture.command(&[
        "patch",
        "approve",
        &proposal,
        "--token",
        &new_token,
        "--dry-run",
    ]);

    assert_eq!(old.status.code(), Some(3));
    assert!(new.status.success());
    assert!(
        !fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl"))
            .unwrap()
            .contains(&old_token)
    );
}

#[test]
fn concurrent_approve_processes_create_one_apply_receipt() {
    let fixture = fixture("concurrent-approve");
    fixture.start();
    let run = fixture.command(&["run", "src/lib.rs 테스트 값을 고쳐줘"]);
    assert!(run.status.success());
    let out = String::from_utf8(run.stdout).unwrap();
    let proposal = field(&out, "proposal id");
    let token = out
        .lines()
        .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
        .unwrap()
        .split(" --token ")
        .nth(1)
        .unwrap()
        .to_string();

    let args = [
        "patch",
        "approve",
        proposal.as_str(),
        "--token",
        token.as_str(),
    ];
    let mut first_command = fixture.command_builder(&args);
    let mut second_command = fixture.command_builder(&args);
    let first = spawn_captured(&mut first_command).unwrap();
    let second = spawn_captured(&mut second_command).unwrap();
    let first = wait_bounded(first, &args);
    let second = wait_bounded(second, &args);
    assert!(first.status.success() || second.status.success());
    let successful_output = if first.status.success() {
        String::from_utf8(first.stdout).unwrap()
    } else {
        String::from_utf8(second.stdout).unwrap()
    };
    let verify_token = verification_token(&successful_output);
    assert_eq!(
        fs::read_to_string(fixture.project.join("src/lib.rs")).unwrap(),
        "pub const VALUE: i32 = 2;\n"
    );
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert_eq!(
        ledger
            .lines()
            .filter(|line| line.contains("\"event_type\":\"patch.applied\""))
            .count(),
        1
    );
    assert_eq!(
        ledger
            .lines()
            .filter(|line| line.contains("\"event_type\":\"verification.evidence.recorded\""))
            .count(),
        0
    );

    let verify = fixture.command(&["patch", "verify", &proposal, "--token", &verify_token]);
    assert!(verify.status.success());
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert_eq!(
        ledger
            .lines()
            .filter(|line| line.contains("\"event_type\":\"verification.evidence.recorded\""))
            .count(),
        1
    );
}
