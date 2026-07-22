use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cli_team_runtime_executes_reconciles_and_retries_without_duplicate_merge() {
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
    let run = fixture.command(&["run", "src/lib.rs 값을 변경해 team runtime parent를 만들어줘"]);
    assert_success(&run, "create parent workflow");
    let parent_id = line_value(&text(&run.stdout), "- workflow id: ").to_string();

    let manifest = format!(
        "{{\"schema_version\":1,\"team_id\":\"team-smoke\",\"parent_workflow_id\":\"{}\",\"members\":[{{\"lane\":1,\"id\":\"explore-1\",\"role\":\"explore\",\"task\":\"inspect the source\",\"tools\":[\"read_file\"],\"read_paths\":[\"src/lib.rs\"],\"write_paths\":[],\"timeout_ms\":5000,\"max_tokens\":128}},{{\"lane\":2,\"id\":\"explore-2\",\"role\":\"explore\",\"task\":\"cross-check the source\",\"tools\":[\"read_file\"],\"read_paths\":[\"src/lib.rs\"],\"write_paths\":[],\"timeout_ms\":5000,\"max_tokens\":128}}],\"write_policy\":\"single_writer\",\"merge_policy\":\"runtime_owned\",\"stop_gate\":\"evidence_required\"}}",
        parent_id
    );
    fs::write(fixture.project.join("team.json"), manifest).unwrap();
    let plan = fixture.command(&["team", "plan", "--manifest", "team.json"]);
    assert_success(&plan, "team plan");
    assert!(text(&plan.stdout).contains("- stage: team-plan"));

    fixture.write_response(
        "{\"schema_version\":1,\"subagent_id\":\"{{SUBAGENT_ID}}\",\"parent_workflow_id\":\"{{PARENT_WORKFLOW_ID}}\",\"role\":\"explore\",\"status\":\"completed\",\"summary\":\"선언 범위 확인 완료\",\"findings\":[\"src/lib.rs 확인\"],\"patch_proposal\":null,\"evidence_refs\":[\"src/lib.rs:1\"],\"validation_gaps\":[],\"suggested_next_action\":\"부모가 결과를 검토\"}",
    );
    let execute = fixture.command(&["team", "execute", "--team", "team-smoke"]);
    assert_success(&execute, "team execute");
    let execute_stdout = text(&execute.stdout);
    assert!(execute_stdout.contains("- status: workers-completed"));
    assert!(execute_stdout.contains("- completed members: 2"));

    let parent_pointer = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{parent_id}.json"));
    let parent_before_merge = fs::read_to_string(&parent_pointer).unwrap();
    let reconcile = fixture.command(&["team", "reconcile", "--team", "team-smoke"]);
    assert_success(&reconcile, "team reconcile");
    let reconcile_stdout = text(&reconcile.stdout);
    assert!(reconcile_stdout.contains("- status: completed"));
    assert!(reconcile_stdout.contains("- stage: complete"));
    assert!(reconcile_stdout.contains("- evidence merged: 2"));
    assert!(reconcile_stdout.contains("- stop gate: passed"));

    let parent_after_merge = fs::read_to_string(&parent_pointer).unwrap();
    assert_ne!(parent_after_merge, parent_before_merge);
    let receipt_path = fixture
        .project
        .join(".rpotato/teams/team-smoke.reconciliation.json");
    let receipt = fs::read_to_string(&receipt_path).unwrap();
    let evidence_ids = json_string_values(&receipt, "evidence_id");
    assert_eq!(evidence_ids.len(), 2);
    let parent_snapshots = fixture
        .project
        .join(".rpotato/workflows")
        .join(format!("{parent_id}.snapshots"));
    for evidence_id in &evidence_ids {
        assert_tree_contains(&parent_snapshots, evidence_id.as_bytes());
    }

    let retry = fixture.command(&["team", "reconcile", "--team", "team-smoke"]);
    assert_success(&retry, "team reconcile retry");
    assert_eq!(
        fs::read_to_string(&parent_pointer).unwrap(),
        parent_after_merge
    );
    assert_eq!(fs::read_to_string(&receipt_path).unwrap(), receipt);

    let status = fixture.command(&["team", "status"]);
    assert_success(&status, "team status");
    assert!(text(&status.stdout).contains("current team stage: complete"));

    let ledger = fs::read_to_string(fixture.data.join("state/runtime-ledger.jsonl")).unwrap();
    assert_stage_order(&ledger);
    for event in [
        "team.result-set.reconciled",
        "team.evidence.merged",
        "team.stop-gate.passed",
        "team.report.completed",
    ] {
        assert_eq!(ledger.matches(event).count(), 1, "ledger={ledger}");
    }
    assert_eq!(ledger.matches("team.worker.completed").count(), 2);
    assert_eq!(ledger.matches("team.worker.action-owned").count(), 2);
    assert_eq!(
        fs::read_to_string(&fixture.requests)
            .unwrap()
            .lines()
            .count(),
        3
    );
}

fn assert_stage_order(ledger: &str) {
    let mut cursor = 0;
    for event in [
        "team.stage.planned",
        "team.stage.dispatched",
        "team.stage.executing",
        "team.stage.reviewing",
        "team.stage.verifying",
        "team.stage.merging",
        "team.stage.reporting",
        "team.stage.completed",
    ] {
        let relative = ledger[cursor..]
            .find(event)
            .unwrap_or_else(|| panic!("missing stage event {event}\n{ledger}"));
        cursor += relative + event.len();
    }
}

fn line_value<'a>(value: &'a str, prefix: &str) -> &'a str {
    value
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("missing output line {prefix}\n{value}"))
}

fn json_string_values(body: &str, key: &str) -> Vec<String> {
    let marker = format!("\"{key}\":\"");
    let mut remaining = body;
    let mut values = Vec::new();
    while let Some(index) = remaining.find(&marker) {
        remaining = &remaining[index + marker.len()..];
        let end = remaining.find('"').unwrap();
        values.push(remaining[..end].to_string());
        remaining = &remaining[end + 1..];
    }
    values
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
    assert!(found, "expected snapshot binding was not found");
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
        let root = std::env::temp_dir().join(format!("rpotato-team-runtime-{nonce}"));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join("src/lib.rs"), "pub const VALUE: i32 = 1;\n").unwrap();
        let backend = root.join(if cfg!(windows) {
            "fake-sidecar.exe"
        } else {
            "fake-sidecar"
        });
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/platform/fake_sidecar.rs");
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

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
