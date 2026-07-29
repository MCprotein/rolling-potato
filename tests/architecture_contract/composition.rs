use super::*;

#[test]
fn v03713_composition_owns_cli_preflight_and_dispatch_ordering() {
    let composition = fs::read_to_string("src/composition/dispatch.rs").unwrap();
    for definition in [
        "trait CommandDispatchPort",
        "fn run(",
        "parser::parse(args)",
        "port.validate_native_terminal()",
        "port.recover_pending_source_bundles()",
        "port.execute(command)",
    ] {
        assert!(
            composition.contains(definition),
            "CLI composition owner is missing {definition}"
        );
    }

    let app = fs::read_to_string("src/app.rs").unwrap();
    assert!(app.contains("dispatch::run(args"));
    assert!(!app.contains("parser::parse(args)"));
    assert!(!app.contains("recover_pending_source_bundles()?"));
    assert!(!app.contains("match command"));

    let adapter = fs::read_to_string("src/app/command_dispatch.rs").unwrap();
    assert!(!Path::new("src/app/legacy_dispatch.rs").exists());
    assert!(!Path::new("src/app/legacy_dispatch").exists());
    assert!(app.lines().any(|line| line == "mod command_dispatch;"));
    assert!(!app.lines().any(|line| line == "mod legacy_dispatch;"));
    assert!(adapter.contains("impl dispatch::CommandDispatchPort for CommandDispatchAdapter"));
    assert!(adapter.contains("match command"));
    assert!(adapter
        .lines()
        .any(|line| line == "mod collaboration_commands;"));
    assert!(adapter
        .lines()
        .any(|line| line == "mod extension_commands;"));
    assert!(adapter.lines().any(|line| line == "mod inference_ports;"));
    assert!(adapter
        .lines()
        .any(|line| line == "mod knowledge_commands;"));
    assert!(adapter
        .lines()
        .any(|line| line == "mod observability_commands;"));
    assert!(adapter.lines().any(|line| line == "mod policy_commands;"));
    assert!(adapter.lines().any(|line| line == "mod tui_commands;"));
    assert!(adapter.lines().any(|line| line == "mod workflow_commands;"));
    let collaboration_commands =
        fs::read_to_string("src/app/command_dispatch/collaboration_commands.rs").unwrap();
    for responsibility in [
        "pub(super) fn execute_team(",
        "pub(super) fn execute_subagent(",
        "TeamCommand::Dispatch",
        "SubagentCommand::Launch",
    ] {
        assert!(collaboration_commands.contains(responsibility));
        assert!(!adapter.contains(responsibility));
    }
    for delegation in [
        "Command::Team(command) => execute_team(command)",
        "Command::Subagent(command) => execute_subagent(command)",
    ] {
        assert!(adapter.contains(delegation));
    }
    let extension_commands =
        fs::read_to_string("src/app/command_dispatch/extension_commands.rs").unwrap();
    for responsibility in [
        "pub(super) fn execute_skill(",
        "pub(super) fn execute_hooks(",
        "pub(super) fn execute_plugin(",
        "PluginCommand::Import",
    ] {
        assert!(extension_commands.contains(responsibility));
        assert!(!adapter.contains(responsibility));
    }
    for delegation in [
        "Command::Skill(command) => execute_skill(command)",
        "Command::Hooks(command) => execute_hooks(command)",
        "Command::Plugin(command) => execute_plugin(command)",
    ] {
        assert!(adapter.contains(delegation));
    }
    let knowledge_commands =
        fs::read_to_string("src/app/command_dispatch/knowledge_commands.rs").unwrap();
    for responsibility in [
        "pub(super) fn execute_evidence(",
        "pub(super) fn execute_ontology(",
        "OntologyCommand::Export",
    ] {
        assert!(knowledge_commands.contains(responsibility));
        assert!(!adapter.contains(responsibility));
    }
    for delegation in [
        "Command::Evidence(command) => execute_evidence(command)",
        "Command::Ontology(command) => execute_ontology(command)",
    ] {
        assert!(adapter.contains(delegation));
    }
    let observability_commands =
        fs::read_to_string("src/app/command_dispatch/observability_commands.rs").unwrap();
    for responsibility in [
        "pub(super) fn execute_cache_status(",
        "pub(super) fn execute_monitor(",
        "MonitorCommand::Export",
    ] {
        assert!(observability_commands.contains(responsibility));
        assert!(!adapter.contains(responsibility));
    }
    for delegation in [
        "Command::CacheStatus => execute_cache_status()",
        "Command::Monitor(command) => execute_monitor(command)",
    ] {
        assert!(adapter.contains(delegation));
    }
    let policy_commands =
        fs::read_to_string("src/app/command_dispatch/policy_commands.rs").unwrap();
    for responsibility in [
        "pub(super) fn execute_policy(",
        "PolicyCommand::CheckPath",
        "PolicyPathMode::Write",
    ] {
        assert!(policy_commands.contains(responsibility));
        assert!(!adapter.contains(responsibility));
    }
    assert!(adapter.contains("Command::Policy(command) => execute_policy(command)"));
    let tui_commands = fs::read_to_string("src/app/command_dispatch/tui_commands.rs").unwrap();
    for responsibility in [
        "pub(super) fn execute_tui(",
        "TuiCommand::Auto",
        "TuiCommand::Interactive",
        "fn print_report(",
    ] {
        assert!(tui_commands.contains(responsibility));
        assert!(!adapter.contains(responsibility));
    }
    assert!(adapter.contains("Command::Tui(command) => execute_tui(command)"));
    let workflow_commands =
        fs::read_to_string("src/app/command_dispatch/workflow_commands.rs").unwrap();
    for responsibility in [
        "pub(super) fn execute_state(",
        "pub(super) fn execute_session(",
        "pub(super) fn execute_patch(",
        "PatchCommand::Approve",
    ] {
        assert!(workflow_commands.contains(responsibility));
        assert!(!adapter.contains(responsibility));
    }
    for delegation in [
        "Command::State(command) => execute_state(command)",
        "Command::Session(command) => execute_session(command)",
        "Command::Patch(command) => execute_patch(command)",
    ] {
        assert!(adapter.contains(delegation));
    }
    let inference_ports =
        fs::read_to_string("src/app/command_dispatch/inference_ports.rs").unwrap();
    for responsibility in [
        "impl inference::BenchmarkCommandPort",
        "impl inference::BackendCommandPort",
        "impl inference::ModelCommandPort",
        "pub(super) fn emit_output(",
    ] {
        assert!(inference_ports.contains(responsibility));
        assert!(!adapter.contains(responsibility));
    }
    assert!(adapter.lines().count() < 130);
    assert!(collaboration_commands.lines().count() < 100);
    assert!(extension_commands.lines().count() < 75);
    assert!(inference_ports.lines().count() < 200);
    assert!(knowledge_commands.lines().count() < 60);
    assert!(observability_commands.lines().count() < 50);
    assert!(policy_commands.lines().count() < 40);
    assert!(tui_commands.lines().count() < 60);
    assert!(workflow_commands.lines().count() < 75);
}

#[test]
fn v03713_composition_owns_benchmark_command_orchestration() {
    let composition = fs::read_to_string("src/composition/inference.rs").unwrap();
    for definition in [
        "trait BenchmarkCommandPort",
        "fn run_benchmark(",
        "BenchmarkCommand::Validate",
        "BenchmarkCommand::Record",
        "BenchmarkCommand::Run",
        "BenchmarkCommand::Report",
        "CommandOutput::Exact",
    ] {
        assert!(
            composition.contains(definition),
            "inference composition owner is missing {definition}"
        );
    }
    for forbidden in ["crate::benchmark", "crate::ledger", "crate::observability"] {
        assert!(
            !composition.contains(forbidden),
            "inference composition bypasses its benchmark port: {forbidden}"
        );
    }

    let adapter = fs::read_to_string("src/app/command_dispatch.rs").unwrap();
    let inference_ports =
        fs::read_to_string("src/app/command_dispatch/inference_ports.rs").unwrap();
    assert!(inference_ports.contains("impl inference::BenchmarkCommandPort"));
    assert!(adapter.contains("inference::run_benchmark(command, self)"));

    assert!(!Path::new("src/benchmark.rs").exists());
    assert!(Path::new("src/app/inference_adapter/benchmark.rs").is_file());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod benchmark;"));
}

#[test]
fn v03713_composition_owns_model_command_orchestration() {
    let composition = fs::read_to_string("src/composition/inference.rs").unwrap();
    for definition in [
        "trait ModelCommandPort",
        "fn run_model(",
        "ModelCommand::List",
        "ModelCommand::Manifest",
        "ModelCommand::Inspect",
        "ModelCommand::SetDefault",
        "ModelCommand::FetchCandidate",
        "ModelCommand::Promote",
        "ModelCommand::Install",
        "CommandOutput::None",
    ] {
        assert!(
            composition.contains(definition),
            "inference composition owner is missing {definition}"
        );
    }
    for forbidden in ["crate::model", "crate::ledger", "crate::observability"] {
        assert!(
            !composition.contains(forbidden),
            "inference composition bypasses its model port: {forbidden}"
        );
    }

    let adapter = fs::read_to_string("src/app/command_dispatch.rs").unwrap();
    let inference_ports =
        fs::read_to_string("src/app/command_dispatch/inference_ports.rs").unwrap();
    assert!(inference_ports.contains("impl inference::ModelCommandPort"));
    assert!(adapter.contains("inference::run_model(command, self)"));

    assert!(!Path::new("src/model.rs").exists());
    assert!(Path::new("src/app/inference_adapter/model.rs").is_file());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod model;"));
}

#[test]
fn v03713_composition_owns_backend_command_orchestration() {
    let composition = fs::read_to_string("src/composition/inference.rs").unwrap();
    for definition in [
        "trait BackendCommandPort",
        "fn run_backend(",
        "BackendCommand::Doctor",
        "BackendCommand::Install",
        "BackendCommand::Start",
        "port.default_model_path()",
        "BackendCommand::VerifyArchive",
        "BackendCommand::Chat",
        "port.chat_stream_report",
        "port.chat_report",
    ] {
        assert!(
            composition.contains(definition),
            "inference composition owner is missing {definition}"
        );
    }
    for forbidden in ["crate::backend", "crate::model", "crate::ledger"] {
        assert!(
            !composition.contains(forbidden),
            "inference composition bypasses its backend port: {forbidden}"
        );
    }

    let adapter = fs::read_to_string("src/app/command_dispatch.rs").unwrap();
    let inference_ports =
        fs::read_to_string("src/app/command_dispatch/inference_ports.rs").unwrap();
    assert!(inference_ports.contains("impl inference::BackendCommandPort"));
    assert!(adapter.contains("inference::run_backend(command, self, &mut writer)"));

    assert!(!Path::new("src/backend.rs").exists());
    assert!(Path::new("src/app/inference_adapter/backend.rs").is_file());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod backend;"));
}
