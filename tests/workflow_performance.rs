#![cfg(unix)]

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BUDGETS: &str = include_str!("../benchmarks/fixtures/workflow-performance-v1.json");
const PATCH_RESPONSE: &str = "MODEL ACTION: kind=patch-proposal; source_pointers=src/lib.rs:1; path=src/lib.rs; find_hex=31; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none";
const READ_ONLY_RESPONSE: &str = "src/lib.rs 구조를 확인했으며 파일 변경은 필요하지 않습니다.\nMODEL ACTION: kind=inspect-sources; source_pointers=src/lib.rs:1; next_gate=source-reread-before-claim; side_effects=none";
const SUBAGENT_RESPONSE: &str = "{\"schema_version\":1,\"subagent_id\":\"{{SUBAGENT_ID}}\",\"parent_workflow_id\":\"{{PARENT_WORKFLOW_ID}}\",\"role\":\"explore\",\"status\":\"completed\",\"summary\":\"선언 범위 확인 완료\",\"findings\":[\"src/lib.rs 확인\"],\"patch_proposal\":null,\"evidence_refs\":[\"src/lib.rs:1\"],\"validation_gaps\":[],\"suggested_next_action\":\"부모가 결과를 검토\"}";

#[test]
fn completed_agent_subagent_and_team_workflows_stay_within_budgets() {
    assert!(BUDGETS.contains("\"claim_state\": \"measured-locally\""));
    assert!(BUDGETS.contains("\"model_claim\": \"not-applicable-fake-sidecar\""));
    assert!(BUDGETS.contains("\"raw_prompt_source_stored\": false"));
    assert_projection_hotspot_closed();

    let agent = measure_agent();
    let subagent = measure_subagent();
    let team = measure_team();

    assert_budget("agent", &agent);
    assert_budget("subagent", &subagent);
    assert_budget("team", &team);

    println!(
        "RPOTATO_WORKFLOW_PERF {{\"fixture_id\":\"workflow-performance-v1\",\"claim_state\":\"measured-locally\",\"agent\":{},\"subagent\":{},\"team\":{},\"raw_prompt_source_stored\":false}}",
        agent.json(),
        subagent.json(),
        team.json(),
    );
}

fn assert_projection_hotspot_closed() {
    let sources = [
        include_str!("../src/app/workflow_adapter/state/workflow_store.rs"),
        include_str!("../src/app/inference_adapter/benchmark.rs"),
        include_str!("../src/app/collaboration_adapter/team_execution/events.rs"),
        include_str!("../src/app/collaboration_adapter/team.rs"),
        include_str!("../src/app/collaboration_adapter/team_state/events.rs"),
        include_str!("../src/app/collaboration_adapter/team_reconciliation.rs"),
    ];
    let supplied_ordinal_calls = sources
        .iter()
        .map(|source| {
            assert!(!source.contains("observability::project_event(&event)"));
            source
                .matches("project_event_with_ordinal(&event, appended.ordinal)")
                .count()
        })
        .sum::<usize>();

    assert_eq!(budget("projection_full_ledger_reads_per_append"), 0);
    assert_eq!(supplied_ordinal_calls, 12);
}

fn measure_agent() -> WorkflowMeasurement {
    let fixture = Fixture::new("agent");
    fixture.start();
    fixture.write_response(READ_ONLY_RESPONSE);

    let run = fixture.measured_command(&["run", "저장소 구조를 분석해줘"]);
    assert_success(&run.output, "completed agent run");
    let stdout = text(&run.output.stdout);
    assert!(stdout.starts_with("run 결과\n- 상태: 완료"), "{stdout}");
    assert!(
        stdout.contains("- action kind: inspect-sources"),
        "{stdout}"
    );
    assert!(stdout.contains("- side effect: 없음"), "{stdout}");

    fixture.measurement("agent", vec![run])
}

fn measure_subagent() -> WorkflowMeasurement {
    let fixture = Fixture::new("subagent");
    fixture.start();
    fixture.write_response(PATCH_RESPONSE);

    let parent = fixture.measured_command(&["run", "src/lib.rs 값을 변경해줘"]);
    assert_success(&parent.output, "subagent parent run");
    assert!(
        text(&parent.output.stdout).contains("- status: pending-approval"),
        "{}",
        text(&parent.output.stdout)
    );

    fixture.write_response(SUBAGENT_RESPONSE);
    let launch = fixture.measured_command(&[
        "subagent",
        "launch",
        "--role",
        "explore",
        "--task",
        "src/lib.rs 선언 범위를 확인해줘",
        "--tool",
        "read_file",
        "--read",
        "src/lib.rs",
        "--timeout-ms",
        "5000",
        "--max-tokens",
        "128",
    ]);
    assert_success(&launch.output, "completed subagent launch");
    let stdout = text(&launch.output.stdout);
    assert!(stdout.contains("- status: completed"), "{stdout}");
    assert!(stdout.contains("- context chars: "), "{stdout}");
    assert!(stdout.contains("- effective max tokens: 128"), "{stdout}");

    fixture.measurement("subagent", vec![parent, launch])
}

fn measure_team() -> WorkflowMeasurement {
    let fixture = Fixture::new("team");
    fixture.start();
    fixture.write_response(PATCH_RESPONSE);

    let parent = fixture.measured_command(&["run", "team runtime 성능 측정 parent"]);
    assert_success(&parent.output, "team parent run");
    let parent_id = line_value(&text(&parent.output.stdout), "- workflow id: ").to_string();
    let manifest = format!(
        "{{\"schema_version\":1,\"team_id\":\"team-performance\",\"parent_workflow_id\":\"{parent_id}\",\"members\":[{{\"lane\":1,\"id\":\"explore-1\",\"role\":\"explore\",\"task\":\"inspect the source\",\"tools\":[\"read_file\"],\"read_paths\":[\"src/lib.rs\"],\"write_paths\":[],\"timeout_ms\":5000,\"max_tokens\":128}},{{\"lane\":2,\"id\":\"explore-2\",\"role\":\"explore\",\"task\":\"cross-check the source\",\"tools\":[\"read_file\"],\"read_paths\":[\"src/lib.rs\"],\"write_paths\":[],\"timeout_ms\":5000,\"max_tokens\":128}}],\"write_policy\":\"single_writer\",\"merge_policy\":\"runtime_owned\",\"stop_gate\":\"evidence_required\"}}"
    );
    fs::write(fixture.project.join("team.json"), manifest).unwrap();

    let plan = fixture.measured_command(&["team", "plan", "--manifest", "team.json"]);
    assert_success(&plan.output, "team plan");
    fixture.write_response(SUBAGENT_RESPONSE);
    let execute = fixture.measured_command(&["team", "execute", "--team", "team-performance"]);
    assert_success(&execute.output, "team execute");
    assert!(
        text(&execute.output.stdout).contains("- completed members: 2"),
        "{}",
        text(&execute.output.stdout)
    );
    let reconcile = fixture.measured_command(&["team", "reconcile", "--team", "team-performance"]);
    assert_success(&reconcile.output, "team reconcile");
    let reconcile_stdout = text(&reconcile.output.stdout);
    assert!(
        reconcile_stdout.contains("- status: completed"),
        "{reconcile_stdout}"
    );
    assert!(
        reconcile_stdout.contains("- stop gate: passed"),
        "{reconcile_stdout}"
    );

    fixture.measurement("team", vec![parent, plan, execute, reconcile])
}

#[derive(Debug)]
struct WorkflowMeasurement {
    name: &'static str,
    command_count: usize,
    wall_ms: u128,
    peak_cpu_percent: Option<f64>,
    peak_rss_bytes: Option<u64>,
    request_count: u64,
    request_bytes: u64,
    total_tokens: u64,
    persisted_bytes: u64,
}

impl WorkflowMeasurement {
    fn json(&self) -> String {
        format!(
            "{{\"commands\":{},\"wall_ms\":{},\"peak_cpu_percent\":{},\"peak_rss_bytes\":{},\"request_count\":{},\"request_bytes\":{},\"total_tokens\":{},\"persisted_bytes\":{}}}",
            self.command_count,
            self.wall_ms,
            option_f64(self.peak_cpu_percent),
            option_u64(self.peak_rss_bytes),
            self.request_count,
            self.request_bytes,
            self.total_tokens,
            self.persisted_bytes,
        )
    }
}

fn assert_budget(name: &str, measurement: &WorkflowMeasurement) {
    assert_eq!(measurement.name, name);
    assert_eq!(
        measurement.request_count,
        budget(&format!("{name}_request_count"))
    );
    assert_eq!(
        measurement.total_tokens,
        budget(&format!("{name}_total_tokens"))
    );
    assert!(
        measurement.request_bytes <= budget(&format!("{name}_max_request_bytes")),
        "{name} request bytes exceeded: {measurement:?}"
    );
    assert!(
        measurement.persisted_bytes <= budget(&format!("{name}_max_persisted_bytes")),
        "{name} persisted bytes exceeded: {measurement:?}"
    );
    assert!(
        measurement.peak_rss_bytes.is_some(),
        "{name} peak RSS was not sampled: {measurement:?}"
    );
}

fn budget(key: &str) -> u64 {
    let marker = format!("\"{key}\":");
    let rest = BUDGETS
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing budget key: {key}"))
        .1
        .trim_start();
    let digits = rest
        .bytes()
        .take_while(u8::is_ascii_digit)
        .map(char::from)
        .collect::<String>();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("invalid budget value: {key}"))
}

struct MeasuredOutput {
    output: Output,
    wall_ms: u128,
    peak_cpu_percent: Option<f64>,
    peak_rss_bytes: Option<u64>,
}

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    data: PathBuf,
    backend: PathBuf,
    model: PathBuf,
    response: PathBuf,
    requests: PathBuf,
    request_sizes: PathBuf,
    port: u16,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rpotato-workflow-performance-{name}-{}-{nonce}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join("src/lib.rs"), "pub const VALUE: i32 = 1;\n").unwrap();
        let backend = root.join("fake-sidecar");
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/platform/fake_sidecar.rs");
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
        let request_sizes = root.join("request-sizes.txt");
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
            request_sizes,
            port,
        }
    }

    fn start(&self) {
        assert_success(&self.command(&["init"]), "init");
        assert_success(
            &self.command(&[
                "backend",
                "start",
                "--model",
                self.model.to_str().unwrap(),
                "--ctx-size",
                "1024",
            ]),
            "backend start",
        );
    }

    fn write_response(&self, response: &str) {
        fs::write(&self.response, response).unwrap();
    }

    fn command_builder(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rpotato"));
        command
            .args(args)
            .env("RPOTATO_PROJECT_ROOT", &self.project)
            .env("RPOTATO_DATA_HOME", &self.data)
            .env("RPOTATO_BACKEND_LLAMA_CPP_PATH", &self.backend)
            .env("RPOTATO_BACKEND_PORT", self.port.to_string())
            .env("RPOTATO_FAKE_RESPONSE_FILE", &self.response)
            .env("RPOTATO_FAKE_REQUEST_MARKER", &self.requests)
            .env("RPOTATO_FAKE_REQUEST_SIZE_MARKER", &self.request_sizes);
        command
    }

    fn command(&self, args: &[&str]) -> Output {
        self.command_builder(args).output().unwrap()
    }

    fn measured_command(&self, args: &[&str]) -> MeasuredOutput {
        let mut command = self.command_builder(args);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        let started = Instant::now();
        let mut peak_cpu_percent: Option<f64> = None;
        let mut peak_rss_bytes: Option<u64> = None;
        loop {
            let (cpu, rss) = sample_process(&child);
            peak_cpu_percent = max_f64(peak_cpu_percent, cpu);
            peak_rss_bytes = max_u64(peak_rss_bytes, rss);
            if child.try_wait().unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let output = child.wait_with_output().unwrap();
        MeasuredOutput {
            output,
            wall_ms: started.elapsed().as_millis(),
            peak_cpu_percent,
            peak_rss_bytes,
        }
    }

    fn measurement(
        &self,
        name: &'static str,
        commands: Vec<MeasuredOutput>,
    ) -> WorkflowMeasurement {
        let baseline = self.command(&["monitor", "baseline"]);
        assert_success(&baseline, "monitor baseline");
        let baseline_stdout = text(&baseline.stdout);
        let request_sizes = fs::read_to_string(&self.request_sizes).unwrap();
        let sizes = request_sizes
            .lines()
            .map(|line| line.parse::<u64>().unwrap())
            .collect::<Vec<_>>();
        WorkflowMeasurement {
            name,
            command_count: commands.len(),
            wall_ms: commands.iter().map(|command| command.wall_ms).sum(),
            peak_cpu_percent: commands.iter().fold(None, |current, command| {
                max_f64(current, command.peak_cpu_percent)
            }),
            peak_rss_bytes: commands.iter().fold(None, |current, command| {
                max_u64(current, command.peak_rss_bytes)
            }),
            request_count: sizes.len() as u64,
            request_bytes: sizes.iter().sum(),
            total_tokens: report_u64(&baseline_stdout, "- total tokens: "),
            persisted_bytes: tree_bytes(&self.project.join(".rpotato"))
                + tree_bytes(&self.data.join("state")),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.command(&["backend", "stop"]);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn sample_process(child: &Child) -> (Option<f64>, Option<u64>) {
    let output = Command::new("ps")
        .args(["-o", "%cpu=", "-o", "rss=", "-p", &child.id().to_string()])
        .output();
    let Ok(output) = output else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }
    let line = text(&output.stdout);
    let mut fields = line.split_whitespace();
    let cpu = fields.next().and_then(|value| value.parse::<f64>().ok());
    let rss = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|kib| kib.saturating_mul(1024));
    (cpu, rss)
}

fn report_u64(report: &str, prefix: &str) -> u64 {
    line_value(report, prefix)
        .parse()
        .unwrap_or_else(|_| panic!("invalid numeric report field: {prefix}\n{report}"))
}

fn line_value<'a>(report: &'a str, prefix: &str) -> &'a str {
    report
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .unwrap_or_else(|| panic!("missing report field: {prefix}\n{report}"))
}

fn tree_bytes(root: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(root) else {
        return 0;
    };
    if metadata.is_file() || metadata.file_type().is_symlink() {
        return metadata.len();
    }
    fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| tree_bytes(&entry.path()))
        .sum()
}

fn max_f64(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn max_u64(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn option_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "null".to_string())
}

fn option_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
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
