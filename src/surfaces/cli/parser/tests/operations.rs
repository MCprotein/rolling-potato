use super::*;

#[test]
fn parses_plugin_import_dry_run() {
    let command = parse([
        "plugin".to_string(),
        "import".to_string(),
        "--from".to_string(),
        "codex".to_string(),
        "./my-plugin".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Plugin(PluginCommand::Import {
            source: PluginSource::Codex,
            path: "./my-plugin".to_string(),
            dry_run: true
        })
    );
}

#[test]
fn parses_monitor_status() {
    let command = parse(["monitor".to_string(), "status".to_string()]).unwrap();
    assert_eq!(command, Command::Monitor(MonitorCommand::Status));
}

#[test]
fn parses_monitor_baseline() {
    let command = parse(["monitor".to_string(), "baseline".to_string()]).unwrap();
    assert_eq!(command, Command::Monitor(MonitorCommand::Baseline));
}

#[test]
fn parses_monitor_optimize() {
    let command = parse(["monitor".to_string(), "optimize".to_string()]).unwrap();
    assert_eq!(command, Command::Monitor(MonitorCommand::Optimize));
}

#[test]
fn parses_benchmark_validate() {
    let command = parse([
        "benchmark".to_string(),
        "validate".to_string(),
        "benchmarks/fixtures/sample.json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Benchmark(BenchmarkCommand::Validate {
            path: "benchmarks/fixtures/sample.json".to_string()
        })
    );
}

#[test]
fn parses_benchmark_record() {
    let command = parse([
        "benchmark".to_string(),
        "record".to_string(),
        "--fixture".to_string(),
        "benchmarks/fixtures/sample.json".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Benchmark(BenchmarkCommand::Record {
            fixture: "benchmarks/fixtures/sample.json".to_string()
        })
    );
}

#[test]
fn parses_benchmark_run() {
    let command = parse([
        "benchmark".to_string(),
        "run".to_string(),
        "--fixture".to_string(),
        "benchmarks/fixtures/executable-smoke.json".to_string(),
        "--prompt".to_string(),
        "benchmarks/prompts/executable-smoke.txt".to_string(),
        "--max-tokens".to_string(),
        "32".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Benchmark(BenchmarkCommand::Run {
            fixture: "benchmarks/fixtures/executable-smoke.json".to_string(),
            prompt: "benchmarks/prompts/executable-smoke.txt".to_string(),
            max_tokens: Some(32)
        })
    );
}

#[test]
fn parses_benchmark_report_jsonl() {
    let command = parse([
        "benchmark".to_string(),
        "report".to_string(),
        "--format".to_string(),
        "jsonl".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Benchmark(BenchmarkCommand::Report {
            format: BenchmarkReportFormat::Jsonl
        })
    );
}

#[test]
fn parses_state_reconcile() {
    let command = parse(["state".to_string(), "reconcile".to_string()]).unwrap();
    assert_eq!(command, Command::State(StateCommand::Reconcile));
}

#[test]
fn parses_state_resume() {
    let command = parse(["state".to_string(), "resume".to_string()]).unwrap();
    assert_eq!(command, Command::State(StateCommand::Resume));
}

#[test]
fn parses_session_list() {
    let command = parse(["session".to_string(), "list".to_string()]).unwrap();
    assert_eq!(command, Command::Session(SessionCommand::List));
}

#[test]
fn parses_session_history_alias() {
    let command = parse(["session".to_string(), "history".to_string()]).unwrap();
    assert_eq!(command, Command::Session(SessionCommand::List));
}

#[test]
fn parses_session_resume() {
    let command = parse([
        "session".to_string(),
        "resume".to_string(),
        "session-1".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Session(SessionCommand::Resume {
            id: "session-1".to_string()
        })
    );
}
