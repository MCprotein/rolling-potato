use super::*;

mod install;
mod tui;
mod uninstall;
mod update;

#[test]
fn parses_subagent_launch_status_and_cancel() {
    let command = parse([
        "subagent".to_string(),
        "launch".to_string(),
        "--role".to_string(),
        "executor".to_string(),
        "--task".to_string(),
        "bounded change".to_string(),
        "--tool".to_string(),
        "read_file".to_string(),
        "--tool".to_string(),
        "render_diff".to_string(),
        "--read".to_string(),
        "src/main.rs".to_string(),
        "--write".to_string(),
        "src/main.rs".to_string(),
        "--timeout-ms".to_string(),
        "1000".to_string(),
        "--max-tokens".to_string(),
        "128".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Subagent(SubagentCommand::Launch {
            role: "executor".to_string(),
            task: "bounded change".to_string(),
            tools: vec!["read_file".to_string(), "render_diff".to_string()],
            read_paths: vec!["src/main.rs".to_string()],
            write_paths: vec!["src/main.rs".to_string()],
            timeout_ms: Some(1000),
            max_tokens: Some(128),
        })
    );
    assert_eq!(
        parse(["subagent".to_string(), "status".to_string()]).unwrap(),
        Command::Subagent(SubagentCommand::Status { id: None })
    );
    assert_eq!(
        parse([
            "subagent".to_string(),
            "cancel".to_string(),
            "subagent-example".to_string(),
        ])
        .unwrap(),
        Command::Subagent(SubagentCommand::Cancel {
            id: "subagent-example".to_string()
        })
    );
}

#[test]
fn subagent_launch_rejects_missing_and_duplicate_singleton_options() {
    let missing = parse([
        "subagent".to_string(),
        "launch".to_string(),
        "--role".to_string(),
        "explore".to_string(),
    ])
    .unwrap_err();
    assert!(missing.message.contains("--task"));

    let duplicate = parse([
        "subagent".to_string(),
        "launch".to_string(),
        "--role".to_string(),
        "explore".to_string(),
        "--role".to_string(),
        "planner".to_string(),
        "--task".to_string(),
        "task".to_string(),
        "--tool".to_string(),
        "read_file".to_string(),
        "--read".to_string(),
        "src/main.rs".to_string(),
    ])
    .unwrap_err();
    assert!(duplicate.message.contains("한 번만"));
}

#[test]
fn parses_ontology_context_query() {
    let command = parse([
        "ontology".to_string(),
        "context".to_string(),
        "--query".to_string(),
        "runtime".to_string(),
        "entrypoint".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Ontology(OntologyCommand::Context {
            query: "runtime entrypoint".to_string()
        })
    );
}

#[test]
fn parses_ontology_reread() {
    let command = parse([
        "ontology".to_string(),
        "reread".to_string(),
        "src/main.rs:1".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Ontology(OntologyCommand::Reread {
            pointer: "src/main.rs:1".to_string()
        })
    );
}

#[test]
fn parses_ontology_export_jsonl() {
    let command = parse([
        "ontology".to_string(),
        "export".to_string(),
        "--format".to_string(),
        "jsonl".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Ontology(OntologyCommand::Export {
            format: OntologyExportFormat::Jsonl
        })
    );
}

#[test]
fn parses_ontology_import_dry_run() {
    let command = parse([
        "ontology".to_string(),
        "import".to_string(),
        "--file".to_string(),
        "ontology-view.jsonl".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Ontology(OntologyCommand::Import {
            path: "ontology-view.jsonl".to_string(),
            dry_run: true
        })
    );
}

#[test]
fn parses_model_install() {
    let command = parse([
        "model".to_string(),
        "install".to_string(),
        "gemma-4-e4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::Install {
            id: "gemma-4-e4b".to_string()
        })
    );
}

#[test]
fn parses_model_manifest() {
    let command = parse(["model".to_string(), "manifest".to_string()]).unwrap();
    assert_eq!(command, Command::Model(ModelCommand::Manifest));
}

#[test]
fn parses_model_inspect() {
    let command = parse([
        "model".to_string(),
        "inspect".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::Inspect {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_registry() {
    let command = parse(["model".to_string(), "registry".to_string()]).unwrap();
    assert_eq!(command, Command::Model(ModelCommand::Registry));
}

#[test]
fn parses_model_default_show_and_select() {
    assert_eq!(
        parse(["model".to_string(), "default".to_string()]).unwrap(),
        Command::Model(ModelCommand::Default)
    );
    assert_eq!(
        parse([
            "model".to_string(),
            "default".to_string(),
            "qwen3.5-4b".to_string(),
        ])
        .unwrap(),
        Command::Model(ModelCommand::SetDefault {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_download_plan() {
    let command = parse([
        "model".to_string(),
        "download-plan".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::DownloadPlan {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_eval_plan() {
    let command = parse([
        "model".to_string(),
        "eval-plan".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::EvalPlan {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_benchmark_plan() {
    let command = parse([
        "model".to_string(),
        "benchmark-plan".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::BenchmarkPlan {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn parses_model_fetch_candidate_for_evaluation() {
    let command = parse([
        "model".to_string(),
        "fetch-candidate".to_string(),
        "qwen3.5-4b".to_string(),
        "--for-evaluation".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::FetchCandidate {
            id: "qwen3.5-4b".to_string()
        })
    );
}

#[test]
fn model_fetch_candidate_requires_evaluation_flag() {
    let err = parse([
        "model".to_string(),
        "fetch-candidate".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("--for-evaluation"));
}

#[test]
fn parses_model_verify_file() {
    let command = parse([
        "model".to_string(),
        "verify-file".to_string(),
        "model.gguf".to_string(),
        "--sha256".to_string(),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::VerifyFile {
            path: "model.gguf".to_string(),
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string()
        })
    );
}

#[test]
fn parses_model_promote_with_evidence_file() {
    let command = parse([
        "model".to_string(),
        "promote".to_string(),
        "qwen3.5-4b".to_string(),
        "--evidence".to_string(),
        "evidence/qwen3.5-4b-local.json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::Promote {
            id: "qwen3.5-4b".to_string(),
            evidence: "evidence/qwen3.5-4b-local.json".to_string()
        })
    );
}

#[test]
fn model_promote_requires_evidence_file() {
    let err = parse([
        "model".to_string(),
        "promote".to_string(),
        "qwen3.5-4b".to_string(),
    ])
    .unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("--evidence"));
}

#[test]
fn parses_model_cleanup_failed_dry_run() {
    let command = parse([
        "model".to_string(),
        "cleanup-failed".to_string(),
        "qwen3.5-4b".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Model(ModelCommand::CleanupFailed {
            id: "qwen3.5-4b".to_string(),
            dry_run: true
        })
    );
}

#[test]
fn parses_backend_doctor() {
    let command = parse(["backend".to_string(), "doctor".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::Doctor));
}

#[test]
fn parses_backend_install_plan() {
    let command = parse(["backend".to_string(), "install-plan".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::InstallPlan));
}

#[test]
fn parses_backend_install() {
    let command = parse(["backend".to_string(), "install".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::Install));
}

#[test]
fn parses_backend_start() {
    let command = parse([
        "backend".to_string(),
        "start".to_string(),
        "--model".to_string(),
        "model.gguf".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::Start {
            model_path: Some("model.gguf".to_string()),
            ctx_size: None
        })
    );
}

#[test]
fn parses_backend_start_without_model_for_default_resolution() {
    let command = parse(["backend".to_string(), "start".to_string()]).unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::Start {
            model_path: None,
            ctx_size: None
        })
    );
}

#[test]
fn parses_backend_start_with_ctx_size() {
    let command = parse([
        "backend".to_string(),
        "start".to_string(),
        "--model".to_string(),
        "model.gguf".to_string(),
        "--ctx-size".to_string(),
        "4096".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::Start {
            model_path: Some("model.gguf".to_string()),
            ctx_size: Some(4096)
        })
    );
}

#[test]
fn rejects_zero_backend_ctx_size() {
    let err = parse([
        "backend".to_string(),
        "start".to_string(),
        "--model".to_string(),
        "model.gguf".to_string(),
        "--ctx-size".to_string(),
        "0".to_string(),
    ])
    .unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("1 이상"));
}

#[test]
fn parses_backend_status() {
    let command = parse(["backend".to_string(), "status".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::Status));
}

#[test]
fn parses_backend_stop() {
    let command = parse(["backend".to_string(), "stop".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::Stop));
}

#[test]
fn parses_backend_verify_archive() {
    let command = parse([
        "backend".to_string(),
        "verify-archive".to_string(),
        "llama.zip".to_string(),
        "--sha256".to_string(),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::VerifyArchive {
            path: "llama.zip".to_string(),
            sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string()
        })
    );
}

#[test]
fn parses_backend_health_check() {
    let command = parse(["backend".to_string(), "health-check".to_string()]).unwrap();
    assert_eq!(command, Command::Backend(BackendCommand::HealthCheck));
}

#[test]
fn parses_backend_chat() {
    let command = parse([
        "backend".to_string(),
        "chat".to_string(),
        "--prompt".to_string(),
        "감자는 무엇인가?".to_string(),
        "--max-tokens".to_string(),
        "64".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Backend(BackendCommand::Chat {
            prompt: "감자는 무엇인가?".to_string(),
            max_tokens: Some(64),
            stream: false,
            timeout_ms: None,
        })
    );
}

#[test]
fn parses_backend_stream_chat_timeout() {
    let command = parse([
        "backend".to_string(),
        "chat".to_string(),
        "--prompt".to_string(),
        "감자".to_string(),
        "--stream".to_string(),
        "--timeout-ms".to_string(),
        "1500".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Backend(BackendCommand::Chat {
            prompt: "감자".to_string(),
            max_tokens: None,
            stream: true,
            timeout_ms: Some(1500),
        })
    );
}

#[test]
fn parses_backend_generation_cancel() {
    let command = parse(["backend".to_string(), "cancel".to_string()]).unwrap();

    assert_eq!(command, Command::Backend(BackendCommand::Cancel));
}

#[test]
fn unknown_backend_command_guidance_includes_cancel() {
    let error = parse(["backend".to_string(), "unknown".to_string()]).unwrap_err();

    assert!(error.message.contains("stop, cancel, verify-archive"));
}

#[test]
fn backend_chat_requires_prompt() {
    let err = parse(["backend".to_string(), "chat".to_string()]).unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("--prompt"));
}

#[test]
fn parses_patch_preview() {
    let command = parse([
        "patch".to_string(),
        "preview".to_string(),
        "--path".to_string(),
        "src/lib.rs".to_string(),
        "--find".to_string(),
        "old".to_string(),
        "--replace".to_string(),
        "new".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Patch(PatchCommand::Preview {
            path: "src/lib.rs".to_string(),
            find: "old".to_string(),
            replace: "new".to_string()
        })
    );
}

#[test]
fn parses_patch_approve_dry_run() {
    let command = parse([
        "patch".to_string(),
        "approve".to_string(),
        "patch-proposal-abc123".to_string(),
        "--token".to_string(),
        "token123".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Patch(PatchCommand::Approve {
            proposal_id: "patch-proposal-abc123".to_string(),
            token: "token123".to_string(),
            dry_run: true
        })
    );
}

#[test]
fn parses_patch_token_rotate() {
    let command = parse([
        "patch".to_string(),
        "token-rotate".to_string(),
        "patch-proposal-wf-example".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Patch(PatchCommand::TokenRotate {
            proposal_id: "patch-proposal-wf-example".to_string()
        })
    );
}

#[test]
fn rejects_patch_approve_with_verify_command() {
    let error = parse([
        "patch".to_string(),
        "approve".to_string(),
        "patch-proposal-abc123".to_string(),
        "--token".to_string(),
        "token123".to_string(),
        "--verify-command".to_string(),
        "cargo fmt --check".to_string(),
    ])
    .unwrap_err();

    assert!(error.message.contains("알 수 없는 patch approve 옵션"));
}

#[test]
fn parses_patch_verify() {
    let command = parse([
        "patch".to_string(),
        "verify".to_string(),
        "patch-proposal-abc123".to_string(),
        "--token".to_string(),
        "token123".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Patch(PatchCommand::Verify {
            proposal_id: "patch-proposal-abc123".to_string(),
            token: "token123".to_string()
        })
    );
}

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

#[test]
fn parses_team_status() {
    let command = parse(["team".to_string(), "status".to_string()]).unwrap();
    assert_eq!(command, Command::Team(TeamCommand::Status));
}

#[test]
fn parses_team_plan_manifest() {
    let command = parse([
        "team".to_string(),
        "plan".to_string(),
        "--manifest".to_string(),
        "plans/team.json".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Plan {
            manifest_path: "plans/team.json".to_string()
        })
    );
}

#[test]
fn team_plan_requires_exactly_one_manifest() {
    for args in [
        vec!["team", "plan"],
        vec!["team", "plan", "--manifest"],
        vec![
            "team",
            "plan",
            "--manifest",
            "one.json",
            "--manifest",
            "two.json",
        ],
    ] {
        assert_eq!(
            parse(args.into_iter().map(str::to_string))
                .unwrap_err()
                .code,
            2
        );
    }
}

#[test]
fn parses_team_execute_id() {
    let command = parse([
        "team".to_string(),
        "execute".to_string(),
        "--team".to_string(),
        "team-execution".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Execute {
            team_id: "team-execution".to_string()
        })
    );
}

#[test]
fn team_execute_requires_exactly_one_id() {
    for args in [
        vec!["team", "execute"],
        vec!["team", "execute", "--team"],
        vec!["team", "execute", "--team", "one", "--team", "two"],
    ] {
        assert_eq!(
            parse(args.into_iter().map(str::to_string))
                .unwrap_err()
                .code,
            2
        );
    }
}

#[test]
fn parses_team_reconcile_id() {
    let command = parse([
        "team".to_string(),
        "reconcile".to_string(),
        "--team".to_string(),
        "team-execution".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Reconcile {
            team_id: "team-execution".to_string()
        })
    );
}

#[test]
fn team_reconcile_requires_exactly_one_id() {
    for args in [
        vec!["team", "reconcile"],
        vec!["team", "reconcile", "--team"],
        vec!["team", "reconcile", "--team", "one", "--team", "two"],
    ] {
        assert_eq!(
            parse(args.into_iter().map(str::to_string))
                .unwrap_err()
                .code,
            2
        );
    }
}

#[test]
fn parses_team_cancel_id() {
    let command = parse([
        "team".to_string(),
        "cancel".to_string(),
        "--team".to_string(),
        "team-execution".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Cancel {
            team_id: "team-execution".to_string()
        })
    );
}

#[test]
fn team_cancel_requires_exactly_one_id() {
    for args in [
        vec!["team", "cancel"],
        vec!["team", "cancel", "--team"],
        vec!["team", "cancel", "--team", "one", "--team", "two"],
    ] {
        assert_eq!(
            parse(args.into_iter().map(str::to_string))
                .unwrap_err()
                .code,
            2
        );
    }
}

#[test]
fn parses_team_admit_with_lanes() {
    let command = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "3".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Admit {
            lanes: 3,
            write_paths: Vec::new(),
            owned_write_paths: Vec::new(),
            commands: Vec::new()
        })
    );
}

#[test]
fn parses_team_admit_policy_preflight() {
    let command = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write".to_string(),
        "README.md".to_string(),
        "--command".to_string(),
        "cargo".to_string(),
        "test".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Admit {
            lanes: 2,
            write_paths: vec!["README.md".to_string()],
            owned_write_paths: Vec::new(),
            commands: vec!["cargo test".to_string()]
        })
    );
}

#[test]
fn parses_team_admit_file_ownership_preflight() {
    let command = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write-owner".to_string(),
        "1:src/app.rs".to_string(),
        "--write-owner".to_string(),
        "2:src/cli.rs".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Admit {
            lanes: 2,
            write_paths: Vec::new(),
            owned_write_paths: vec![(1, "src/app.rs".to_string()), (2, "src/cli.rs".to_string())],
            commands: Vec::new()
        })
    );
}

#[test]
fn parses_team_dispatch_file_ownership_preflight() {
    let command = parse([
        "team".to_string(),
        "dispatch".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write-owner".to_string(),
        "1:src/app.rs".to_string(),
        "--write-owner".to_string(),
        "2:src/cli.rs".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Dispatch {
            lanes: 2,
            owned_write_paths: vec![(1, "src/app.rs".to_string()), (2, "src/cli.rs".to_string())],
            failed_lane: None,
            failure_reason: None,
        })
    );
}

#[test]
fn parses_team_dispatch_failed_lane_continuation() {
    let command = parse([
        "team".to_string(),
        "dispatch".to_string(),
        "--lanes".to_string(),
        "3".to_string(),
        "--write-owner".to_string(),
        "1:src/app.rs".to_string(),
        "--write-owner".to_string(),
        "2:src/cli.rs".to_string(),
        "--failed-lane".to_string(),
        "2".to_string(),
        "--failure".to_string(),
        "worker".to_string(),
        "timed".to_string(),
        "out".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Dispatch {
            lanes: 3,
            owned_write_paths: vec![(1, "src/app.rs".to_string()), (2, "src/cli.rs".to_string())],
            failed_lane: Some(2),
            failure_reason: Some("worker timed out".to_string()),
        })
    );
}

#[test]
fn parses_team_governor() {
    let command = parse([
        "team".to_string(),
        "governor".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--context-tokens".to_string(),
        "6000".to_string(),
        "--context-limit".to_string(),
        "8192".to_string(),
        "--model-tier".to_string(),
        "standard".to_string(),
    ])
    .unwrap();
    assert_eq!(
        command,
        Command::Team(TeamCommand::Governor {
            lanes: 2,
            context_tokens: 6000,
            context_limit: Some(8192),
            model_tier: ModelTier::Standard
        })
    );
}

#[test]
fn rejects_unknown_team_governor_model_tier() {
    let err = parse([
        "team".to_string(),
        "governor".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--context-tokens".to_string(),
        "6000".to_string(),
        "--model-tier".to_string(),
        "frontier".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("small, standard, large"));
}

#[test]
fn rejects_team_admit_write_owner_outside_requested_lanes() {
    let err = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write-owner".to_string(),
        "3:src/app.rs".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("--lanes 2"));
}

#[test]
fn rejects_team_dispatch_without_write_owner() {
    let err = parse([
        "team".to_string(),
        "dispatch".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("--write-owner"));
}

#[test]
fn rejects_team_dispatch_failure_without_failed_lane() {
    let err = parse([
        "team".to_string(),
        "dispatch".to_string(),
        "--lanes".to_string(),
        "2".to_string(),
        "--write-owner".to_string(),
        "1:src/app.rs".to_string(),
        "--failure".to_string(),
        "worker".to_string(),
        "timed".to_string(),
        "out".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("--failed-lane"));
}

#[test]
fn rejects_zero_team_admit_lanes() {
    let err = parse([
        "team".to_string(),
        "admit".to_string(),
        "--lanes".to_string(),
        "0".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.code, 2);
    assert!(err.message.contains("1 이상"));
}

#[test]
fn parses_top_level_resume_as_history() {
    let command = parse(["resume".to_string()]).unwrap();
    assert_eq!(command, Command::Session(SessionCommand::List));
}

#[test]
fn parses_top_level_resume_with_id() {
    let command = parse(["resume".to_string(), "session-1".to_string()]).unwrap();
    assert_eq!(
        command,
        Command::Session(SessionCommand::Resume {
            id: "session-1".to_string()
        })
    );
}

#[test]
fn parses_top_level_continue_as_current_workflow_resume() {
    let command = parse(["continue".to_string()]).unwrap();
    assert_eq!(command, Command::State(StateCommand::Resume));
}

#[test]
fn parses_top_level_continue_with_session_id() {
    let command = parse(["continue".to_string(), "session-1".to_string()]).unwrap();
    assert_eq!(
        command,
        Command::Session(SessionCommand::Resume {
            id: "session-1".to_string()
        })
    );
}

#[test]
fn parses_debug_help_as_advanced_help() {
    assert_eq!(
        parse(["debug".to_string(), "--help".to_string()]).unwrap(),
        Command::AdvancedHelp
    );
}

#[test]
fn parses_existing_commands_beneath_debug_namespace() {
    assert_eq!(
        parse([
            "debug".to_string(),
            "backend".to_string(),
            "status".to_string(),
        ])
        .unwrap(),
        Command::Backend(BackendCommand::Status)
    );
}

#[test]
fn parses_evidence_validate() {
    let command = parse([
        "evidence".to_string(),
        "validate".to_string(),
        "logs/test.log".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Evidence(EvidenceCommand::Validate {
            pointer: "logs/test.log".to_string()
        })
    );
}

#[test]
fn parses_skill_run() {
    let command = parse([
        "skill".to_string(),
        "run".to_string(),
        "fix-test".to_string(),
        "테스트".to_string(),
        "고쳐줘".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Skill(SkillCommand::Run {
            id: "fix-test".to_string(),
            request: "테스트 고쳐줘".to_string()
        })
    );
}

#[test]
fn skill_run_requires_request() {
    let error = parse([
        "skill".to_string(),
        "run".to_string(),
        "fix-test".to_string(),
    ])
    .unwrap_err();

    assert_eq!(error.code, 2);
    assert!(error.message.contains("request 문자열"));
}

#[test]
fn parses_run_request() {
    let command = parse([
        "run".to_string(),
        "테스트".to_string(),
        "고쳐줘".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Run {
            request: "테스트 고쳐줘".to_string()
        }
    );
}

#[test]
fn parses_intent_classify_request() {
    let command = parse([
        "intent".to_string(),
        "classify".to_string(),
        "리뷰해줘".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Intent(IntentCommand::Classify {
            request: "리뷰해줘".to_string()
        })
    );
}

#[test]
fn parses_intent_routes() {
    let command = parse(["intent".to_string(), "routes".to_string()]).unwrap();
    assert_eq!(command, Command::Intent(IntentCommand::Routes));
}

#[test]
fn parses_policy_check_command() {
    let command = parse([
        "policy".to_string(),
        "check-command".to_string(),
        "cargo".to_string(),
        "test".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Policy(PolicyCommand::CheckCommand {
            command: "cargo test".to_string()
        })
    );
}

#[test]
fn parses_policy_check_path_write() {
    let command = parse([
        "policy".to_string(),
        "check-path".to_string(),
        "--write".to_string(),
        "src/main.rs".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Policy(PolicyCommand::CheckPath {
            mode: PolicyPathMode::Write,
            path: "src/main.rs".to_string()
        })
    );
}

#[test]
fn parses_hooks_list() {
    let command = parse(["hooks".to_string(), "list".to_string()]).unwrap();
    assert_eq!(command, Command::Hooks(HooksCommand::List));
}

#[test]
fn parses_monitor_export_jsonl() {
    let command = parse([
        "monitor".to_string(),
        "export".to_string(),
        "--format".to_string(),
        "jsonl".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Monitor(MonitorCommand::Export {
            format: MonitorExportFormat::Jsonl
        })
    );
}

#[test]
fn parses_monitor_export_html() {
    let command = parse([
        "monitor".to_string(),
        "export".to_string(),
        "--format".to_string(),
        "html".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Monitor(MonitorCommand::Export {
            format: MonitorExportFormat::Html
        })
    );
}

#[test]
fn parses_monitor_prune_dry_run() {
    let command = parse([
        "monitor".to_string(),
        "prune".to_string(),
        "--before".to_string(),
        "30d".to_string(),
        "--dry-run".to_string(),
    ])
    .unwrap();

    assert_eq!(
        command,
        Command::Monitor(MonitorCommand::Prune {
            before_days: 30,
            dry_run: true
        })
    );
}
