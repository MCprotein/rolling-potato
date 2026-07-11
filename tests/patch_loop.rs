#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    }
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    data: PathBuf,
    backend: PathBuf,
    response: PathBuf,
    calls: PathBuf,
}

impl Fixture {
    fn command_builder(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rpotato"));
        command
            .args(args)
            .env("RPOTATO_PROJECT_ROOT", &self.project)
            .env("RPOTATO_DATA_HOME", &self.data)
            .env("RPOTATO_BACKEND_LLAMA_CPP_PATH", &self.backend)
            .env("RPOTATO_BACKEND_PORT", port_for(&self.root).to_string());
        command
    }

    fn command(&self, args: &[&str]) -> Output {
        let mut command = self.command_builder(args);
        let child = spawn_captured(&mut command).unwrap();
        wait_bounded(child, args)
    }

    fn start(&self) {
        fs::write(self.root.join("model.gguf"), b"fake model").unwrap();
        let output = self.command(&[
            "backend",
            "start",
            "--model",
            self.root.join("model.gguf").to_str().unwrap(),
            "--ctx-size",
            "1024",
        ]);
        if !output.status.success() {
            let logs = fs::read_dir(self.data.join("logs"))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .filter_map(|entry| fs::read_to_string(entry.path()).ok())
                .collect::<Vec<_>>()
                .join("\n");
            panic!("{}\n{logs}", String::from_utf8_lossy(&output.stderr));
        }
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

fn port_for(path: &Path) -> u16 {
    let hash = path.display().to_string().bytes().fold(0_u16, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u16)
    });
    30000 + (hash % 20000)
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
fn read_only_run_completes_without_patch_gate() {
    let fixture = fixture("read-only-subprocess");
    fs::write(
        &fixture.response,
        "구조를 확인했으며 파일 변경은 필요하지 않습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none",
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
    assert!(report.contains("구조를 확인했으며 파일 변경은 필요하지 않습니다."));
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
fn streaming_guard_blocks_forbidden_model_bytes_before_output_or_success_ledger() {
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

    assert_eq!(chat.status.code(), Some(3));
    assert!(!String::from_utf8_lossy(&chat.stdout).contains(forbidden));
    assert!(!String::from_utf8_lossy(&chat.stderr).contains(forbidden));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("backend.generation.failed"));
    assert!(!ledger.contains("backend.chat.completed"));
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
fn streaming_guard_emits_prior_korean_but_never_later_forbidden_unit() {
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

    assert_eq!(chat.status.code(), Some(3));
    let stdout = String::from_utf8_lossy(&chat.stdout);
    assert!(stdout.contains("정상 한국어 문장입니다."));
    assert!(!stdout.contains(forbidden));
    assert!(!String::from_utf8_lossy(&chat.stderr).contains(forbidden));
    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert!(ledger.contains("backend.generation.failed"));
    assert!(!ledger.contains("backend.chat.completed"));
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
        "5000",
    ]);
    let chat = spawn_captured(&mut command).unwrap();
    wait_for_path(
        &fixture.data.join("state/backend-active-generation.txt"),
        Duration::from_secs(2),
    );

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
