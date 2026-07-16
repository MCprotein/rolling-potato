use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const RAW_TASK: &str = "RPOTATO_SUBAGENT_RAW_TASK_MUST_NOT_PERSIST";
const MODEL_SECRET: &str = "RPOTATO_MODEL_SECRET_MUST_NOT_PERSIST";

#[test]
fn cli_subagent_lifecycle_is_bounded_deterministic_and_secret_safe() {
    let fixture = Fixture::new();
    assert_success(&fixture.command(&["init"]), "init");
    assert_success(
        &fixture.command(&[
            "backend",
            "start",
            "--model",
            fixture.model.to_str().unwrap(),
            "--ctx-size",
            "1024",
        ]),
        "backend start",
    );

    fixture.write_response(
        "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none",
    );
    let parent = fixture.command(&["run", "src/lib.rs 값을 변경해줘"]);
    assert_success(&parent, "create active parent workflow");

    fixture.write_response(
        "{\"schema_version\":1,\"subagent_id\":\"{{SUBAGENT_ID}}\",\"parent_workflow_id\":\"{{PARENT_WORKFLOW_ID}}\",\"role\":\"explore\",\"status\":\"completed\",\"summary\":\"선언 범위 확인 완료\",\"findings\":[\"src/lib.rs 확인\"],\"patch_proposal\":null,\"evidence_refs\":[\"src/lib.rs:1\"],\"validation_gaps\":[],\"suggested_next_action\":\"부모가 결과를 검토\"}",
    );
    let launch = fixture.command(&[
        "subagent",
        "launch",
        "--role",
        "explore",
        "--task",
        RAW_TASK,
        "--tool",
        "read_file",
        "--read",
        "src/lib.rs",
        "--timeout-ms",
        "5000",
        "--max-tokens",
        "128",
    ]);
    assert_success(&launch, "subagent launch");
    let launch_stdout = text(&launch.stdout);
    assert!(launch_stdout.contains("- status: completed"));
    assert!(!launch_stdout.contains(RAW_TASK));

    let subagent_id = line_value(&launch_stdout, "- subagent id: ");
    let parent_id = line_value(&launch_stdout, "- parent workflow: ");
    let result_id = line_value(&launch_stdout, "- result artifact: ");
    let evidence_id = line_value(&launch_stdout, "- evidence: ");
    let child_path = fixture
        .project
        .join(".rpotato/subagents")
        .join(format!("{subagent_id}.json"));
    let parent_path = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{parent_id}.json"));
    let parent_snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{parent_id}.snapshots"));
    let result_path = fixture
        .project
        .join(".rpotato/subagent-results")
        .join(format!("{result_id}.json"));
    let evidence_path = fixture
        .project
        .join(".rpotato/evidence")
        .join(format!("{evidence_id}.json"));
    let ledger_path = fixture.data.join("state/runtime-ledger.jsonl");

    let child_before = fs::read_to_string(&child_path).unwrap();
    let parent_before = fs::read_to_string(&parent_path).unwrap();
    let ledger_before = fs::read_to_string(&ledger_path).unwrap();
    assert!(child_before.contains("\"revision\":4"));
    assert!(child_before.contains("\"status\":\"completed\""));
    assert_tree_contains(&parent_snapshots, evidence_id.as_bytes());
    assert!(result_path.is_file());
    assert!(evidence_path.is_file());
    assert_event_order(&ledger_before);

    let status = fixture.command(&["subagent", "status", subagent_id]);
    assert_success(&status, "subagent status");
    let status_stdout = text(&status.stdout);
    for expected in [
        "- action: read-only",
        "- requested max tokens: 128",
        "- effective max tokens: 128",
        "- backend event: event-",
        &format!("- evidence: {evidence_id}"),
        "- failure code: 없음",
    ] {
        assert!(status_stdout.contains(expected), "status={status_stdout}");
    }
    assert_unchanged(&child_path, &child_before);
    assert_unchanged(&parent_path, &parent_before);
    assert_unchanged(&ledger_path, &ledger_before);

    let cancel = fixture.command(&["subagent", "cancel", subagent_id]);
    assert_success(&cancel, "terminal subagent cancel");
    assert!(text(&cancel.stdout).contains("- action: terminal-preserved-no-op"));
    assert_unchanged(&child_path, &child_before);
    assert_unchanged(&parent_path, &parent_before);
    assert_unchanged(&ledger_path, &ledger_before);

    let result_count = file_count(&fixture.project.join(".rpotato/subagent-results"));
    let evidence_count = file_count(&fixture.project.join(".rpotato/evidence"));
    fixture.write_response(&format!(
        "{{\"schema_version\":1,\"subagent_id\":\"{{{{SUBAGENT_ID}}}}\",\"parent_workflow_id\":\"{{{{PARENT_WORKFLOW_ID}}}}\",\"role\":\"explore\",\"status\":\"completed\",\"summary\":\"token={MODEL_SECRET}\",\"findings\":[\"src/lib.rs 확인\"],\"patch_proposal\":null,\"evidence_refs\":[\"src/lib.rs:1\"],\"validation_gaps\":[],\"suggested_next_action\":\"부모가 결과를 검토\"}}"
    ));
    let sensitive = fixture.command(&[
        "subagent",
        "launch",
        "--role",
        "explore",
        "--task",
        "민감 결과 차단 검사",
        "--tool",
        "read_file",
        "--read",
        "src/lib.rs",
        "--timeout-ms",
        "5000",
        "--max-tokens",
        "128",
    ]);
    assert_failure(&sensitive, "sensitive subagent result");
    let sensitive_output = format!("{}{}", text(&sensitive.stdout), text(&sensitive.stderr));
    assert!(sensitive_output.contains("sensitive output 차단"));
    assert!(!sensitive_output.contains(MODEL_SECRET));
    assert_eq!(
        file_count(&fixture.project.join(".rpotato/subagent-results")),
        result_count
    );
    assert_eq!(
        file_count(&fixture.project.join(".rpotato/evidence")),
        evidence_count
    );

    assert_eq!(
        fs::read_to_string(&fixture.requests)
            .unwrap()
            .lines()
            .count(),
        3
    );
    assert_tree_omits(&fixture.project.join(".rpotato"), RAW_TASK.as_bytes());
    assert_tree_omits(&fixture.data.join("state"), RAW_TASK.as_bytes());
    assert_tree_omits(&fixture.project.join(".rpotato"), MODEL_SECRET.as_bytes());
    assert_tree_omits(&fixture.data.join("state"), MODEL_SECRET.as_bytes());
    assert!(!text(&launch.stderr).contains(RAW_TASK));
    assert!(!status_stdout.contains(RAW_TASK));
}

fn assert_event_order(ledger: &str) {
    let mut cursor = 0;
    for event in [
        "team.subagent.requested",
        "team.subagent.admitted",
        "team.subagent.started",
        "team.subagent.completed",
        "team.subagent.result-merged",
    ] {
        assert_eq!(ledger.matches(event).count(), 1, "ledger={ledger}");
        let relative = ledger[cursor..]
            .find(event)
            .expect("missing lifecycle event");
        cursor += relative + event.len();
    }
}

fn line_value<'a>(text: &'a str, prefix: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("missing output line: {prefix}\n{text}"))
}

fn assert_unchanged(path: &Path, expected: &str) {
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        expected,
        "path={}",
        path.display()
    );
}

fn assert_tree_omits(root: &Path, needle: &[u8]) {
    let entries = fs::read_dir(root).unwrap();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            assert_tree_omits(&path, needle);
        } else {
            let bytes = fs::read(&path).unwrap();
            assert!(
                !bytes.windows(needle.len()).any(|window| window == needle),
                "raw task leaked to {}",
                path.display()
            );
        }
    }
}

fn assert_tree_contains(root: &Path, needle: &[u8]) {
    let found = fs::read_dir(root).unwrap().any(|entry| {
        let path = entry.unwrap().path();
        path.is_file()
            && fs::read(path)
                .unwrap()
                .windows(needle.len())
                .any(|window| window == needle)
    });
    assert!(found, "expected canonical snapshot binding was not found");
}

fn file_count(root: &Path) -> usize {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_file())
                .count()
        })
        .unwrap_or(0)
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    data: PathBuf,
    backend: PathBuf,
    model: PathBuf,
    response: PathBuf,
    requests: PathBuf,
    port: u16,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rpotato-subagent-lifecycle-{nonce}"));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join("src/lib.rs"), "pub const VALUE: i32 = 1;\n").unwrap();
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
        let response = root.join("response.txt");
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
            response,
            requests,
            port,
        }
    }

    fn write_response(&self, response: &str) {
        fs::write(&self.response, response).unwrap();
    }

    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rpotato"))
            .args(args)
            .env("RPOTATO_PROJECT_ROOT", &self.project)
            .env("RPOTATO_DATA_HOME", &self.data)
            .env("RPOTATO_BACKEND_LLAMA_CPP_PATH", &self.backend)
            .env("RPOTATO_BACKEND_PORT", self.port.to_string())
            .env("RPOTATO_FAKE_RESPONSE_FILE", &self.response)
            .env("RPOTATO_FAKE_REQUEST_MARKER", &self.requests)
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.command(&["backend", "stop"]);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout={}\nstderr={}",
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn assert_failure(output: &Output, label: &str) {
    assert!(
        !output.status.success(),
        "{label} unexpectedly succeeded\nstdout={}\nstderr={}",
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
