use super::*;

#[test]
fn v03713_cli_surface_owners_replace_legacy_module() {
    let owner = fs::read_to_string("src/surfaces/cli/command.rs").unwrap();
    for definition in [
        "pub enum Command",
        "pub enum TeamCommand",
        "pub enum BackendCommand",
        "pub enum PluginCommand",
        "pub enum UninstallCommand",
    ] {
        assert!(
            owner.contains(definition),
            "CLI command owner is missing definition: {definition}"
        );
    }

    let parser_path = "src/surfaces/cli/parser.rs";
    let backend_parser_path = "src/surfaces/cli/parser/backend.rs";
    let collaboration_parser_path = "src/surfaces/cli/parser/collaboration.rs";
    let observability_parser_path = "src/surfaces/cli/parser/observability.rs";
    let patch_parser_path = "src/surfaces/cli/parser/patch.rs";
    let plugin_parser_path = "src/surfaces/cli/parser/plugin.rs";
    let uninstall_parser_path = "src/surfaces/cli/parser/uninstall.rs";
    let parser_tests_path = "src/surfaces/cli/parser/tests/mod.rs";
    let uninstall_tests_path = "src/surfaces/cli/parser/tests/uninstall.rs";
    assert!(Path::new(backend_parser_path).is_file());
    assert!(Path::new(collaboration_parser_path).is_file());
    assert!(Path::new(observability_parser_path).is_file());
    assert!(Path::new(patch_parser_path).is_file());
    assert!(Path::new(plugin_parser_path).is_file());
    assert!(Path::new(uninstall_parser_path).is_file());
    assert!(Path::new(parser_tests_path).is_file());
    assert!(Path::new(uninstall_tests_path).is_file());
    let parser = fs::read_to_string(parser_path).unwrap();
    let backend_parser = fs::read_to_string(backend_parser_path).unwrap();
    let collaboration_parser = fs::read_to_string(collaboration_parser_path).unwrap();
    let observability_parser = fs::read_to_string(observability_parser_path).unwrap();
    let patch_parser = fs::read_to_string(patch_parser_path).unwrap();
    let plugin_parser = fs::read_to_string(plugin_parser_path).unwrap();
    let uninstall_parser = fs::read_to_string(uninstall_parser_path).unwrap();
    let parser_tests = fs::read_to_string(parser_tests_path).unwrap();
    let uninstall_tests = fs::read_to_string(uninstall_tests_path).unwrap();
    assert!(parser.contains("pub fn parse"));
    assert!(parser.contains("surfaces::cli::command::*"));
    assert!(
        parser.lines().any(|line| line == "mod backend;"),
        "CLI parser does not register the backend command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod collaboration;"),
        "CLI parser does not register the collaboration command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod observability;"),
        "CLI parser does not register the observability command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod patch;"),
        "CLI parser does not register the patch command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod plugin;"),
        "CLI parser does not register the plugin command-family owner"
    );
    assert!(
        parser.lines().any(|line| line == "mod uninstall;"),
        "CLI parser does not register the uninstall command-family owner"
    );
    for responsibility in [
        "pub(super) fn parse_backend_start(",
        "pub(super) fn parse_backend_chat(",
    ] {
        assert!(
            !parser.contains(responsibility),
            "backend parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            backend_parser.contains(responsibility),
            "backend parser is missing responsibility: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn parse_team_plan_args(",
        "pub(super) fn parse_team_admit_args(",
        "pub(super) fn parse_team_dispatch_args(",
        "pub(super) fn parse_team_governor_args(",
        "pub(super) fn parse_subagent_launch_args(",
    ] {
        assert!(
            !parser.contains(responsibility),
            "collaboration parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            collaboration_parser.contains(responsibility),
            "collaboration parser is missing responsibility: {responsibility}"
        );
    }
    let plugin_parser_responsibility = "pub(super) fn parse_plugin_import(";
    assert!(
        !parser.contains(plugin_parser_responsibility),
        "plugin parser responsibility escaped into CLI facade: {plugin_parser_responsibility}"
    );
    assert!(
        plugin_parser.contains(plugin_parser_responsibility),
        "plugin parser is missing responsibility: {plugin_parser_responsibility}"
    );
    let uninstall_parser_responsibility = "pub(super) fn parse_uninstall(";
    assert!(
        !parser.contains(uninstall_parser_responsibility),
        "uninstall parser responsibility escaped into CLI facade: {uninstall_parser_responsibility}"
    );
    assert!(
        uninstall_parser.contains(uninstall_parser_responsibility),
        "uninstall parser is missing responsibility: {uninstall_parser_responsibility}"
    );
    for responsibility in [
        "pub(super) fn parse_patch_preview(",
        "pub(super) fn parse_patch_approve(",
        "pub(super) fn parse_patch_verify(",
    ] {
        assert!(
            !parser.contains(responsibility),
            "patch parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            patch_parser.contains(responsibility),
            "patch parser is missing responsibility: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn parse_monitor_export(",
        "pub(super) fn parse_monitor_prune(",
        "pub(super) fn parse_ontology_context(",
        "pub(super) fn parse_ontology_import(",
        "pub(super) fn parse_benchmark_run(",
        "pub(super) fn parse_benchmark_report(",
    ] {
        assert!(
            !parser.contains(responsibility),
            "observability parser responsibility escaped into CLI facade: {responsibility}"
        );
        assert!(
            observability_parser.contains(responsibility),
            "observability parser is missing responsibility: {responsibility}"
        );
    }
    assert!(parser.contains("#[path = \"parser/tests/mod.rs\"]"));
    for responsibility in [
        "fn parses_subagent_launch_status_and_cancel(",
        "fn parses_backend_chat(",
        "fn parses_patch_approve_dry_run(",
        "fn parses_team_governor(",
    ] {
        assert!(
            parser_tests.contains(responsibility),
            "CLI parser regression tests are missing responsibility: {responsibility}"
        );
    }
    assert!(uninstall_tests.contains("fn parses_uninstall_dry_run_purge_cache("));
    assert!(uninstall_tests.contains("fn parses_guarded_clean_uninstall("));
    assert!(
        parser.lines().count() < 590,
        "CLI parser production module regrew beyond its command-family extraction boundary"
    );
    assert!(
        backend_parser.lines().count() < 160,
        "backend parser regrew beyond its ownership boundary"
    );
    assert!(
        collaboration_parser.lines().count() < 550,
        "collaboration parser regrew beyond its ownership boundary"
    );
    assert!(
        observability_parser.lines().count() < 300,
        "observability parser regrew beyond its ownership boundary"
    );
    assert!(
        patch_parser.lines().count() < 170,
        "patch parser regrew beyond its ownership boundary"
    );
    assert!(
        plugin_parser.lines().count() < 80,
        "plugin parser regrew beyond its ownership boundary"
    );
    assert!(
        parser_tests.lines().count() < 1_500,
        "CLI parser regression module regrew beyond its ownership boundary"
    );

    let render = fs::read_to_string("src/surfaces/cli/render.rs").unwrap();
    assert!(render.contains("const HELP"));
    assert!(!parser.contains("const HELP"));

    assert!(
        !Path::new("src/cli.rs").exists(),
        "legacy CLI module remains after surface migration"
    );
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod cli;"));
}

#[test]
fn v03713_binary_entrypoint_delegates_process_outcome_to_startup() {
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(main.contains("composition::startup::run"));
    assert!(!main.contains("pub fn run"));
    assert!(!Path::new("src/lib.rs").exists());
    assert!(!main.contains("eprintln!"));
    assert!(!main.contains("match app::run"));

    let startup = fs::read_to_string("src/composition/startup.rs").unwrap();
    assert!(startup.contains("startup_error_message(&err)"));
    assert!(!startup.contains("korean_guard::guard_or_failure"));
    assert!(startup.contains("ExitCode::from(err.code)"));
}

#[test]
fn v03713_uninstall_plan_uses_composition_and_filesystem_owners() {
    let composition = fs::read_to_string("src/composition/uninstall.rs").unwrap();
    assert!(composition.contains("uninstall::managed_paths"));
    assert!(composition.contains("pub(crate) fn plan_report"));
    assert!(composition.contains("pub(crate) fn uninstall_report"));
    assert!(composition.contains("runtime_mutation::acquire(\"clean uninstall\")"));
    assert!(composition.contains("install::require_inactive_runtime(\"clean uninstall\")"));

    let adapter = fs::read_to_string("src/adapters/filesystem/uninstall.rs").unwrap();
    assert!(adapter.contains("struct ManagedUninstallPaths"));
    assert!(adapter.contains("pub(crate) fn managed_paths"));

    assert!(!Path::new("src/uninstall.rs").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod uninstall;"));
}

#[test]
fn v0420_install_ux_has_owned_cli_composition_and_adapter_boundaries() {
    let command = fs::read_to_string("src/surfaces/cli/command.rs").unwrap();
    assert!(command.contains("pub enum InstallCommand"));
    assert!(command.contains("Install(InstallCommand)"));
    assert!(command.contains("CleanDryRun"));
    assert!(command.contains("CleanConfirmed"));

    let parser = fs::read_to_string("src/surfaces/cli/parser.rs").unwrap();
    let install_parser = fs::read_to_string("src/surfaces/cli/parser/install.rs").unwrap();
    let install_tests = fs::read_to_string("src/surfaces/cli/parser/tests/install.rs").unwrap();
    let uninstall_parser = fs::read_to_string("src/surfaces/cli/parser/uninstall.rs").unwrap();
    let uninstall_tests = fs::read_to_string("src/surfaces/cli/parser/tests/uninstall.rs").unwrap();
    assert!(parser.lines().any(|line| line == "mod install;"));
    assert!(parser.contains("use install::parse_install;"));
    assert!(install_parser.contains("pub(super) fn parse_install("));
    assert!(install_tests.contains("fn parses_standard_and_guarded_clean_install("));
    assert!(install_tests.contains("fn clean_install_requires_exactly_one_safety_mode("));
    assert!(parser.lines().any(|line| line == "mod uninstall;"));
    assert!(parser.contains("use uninstall::parse_uninstall;"));
    assert!(uninstall_parser.contains("pub(super) fn parse_uninstall("));
    assert!(uninstall_tests.contains("fn parses_guarded_clean_uninstall("));

    let composition = fs::read_to_string("src/composition/install.rs").unwrap();
    assert!(composition.contains("pub(crate) fn install_report("));
    assert!(composition.contains("pub(crate) fn init_environment_report("));
    assert!(composition.contains("require_inactive_runtime"));
    assert!(composition.contains("backend_process::running_status"));
    assert!(composition.contains("runtime_mutation::acquire(\"clean install\")"));

    let adapter = fs::read_to_string("src/adapters/system_install.rs").unwrap();
    let uninstall_adapter = fs::read_to_string("src/adapters/system_install/uninstall.rs").unwrap();
    let adapter_tests = fs::read_to_string("src/adapters/system_install/tests.rs").unwrap();
    assert!(adapter.lines().any(|line| line == "mod uninstall;"));
    assert!(adapter.contains("pub(crate) fn install_binary("));
    assert!(adapter.contains("pub(crate) fn binary_install_plan("));
    assert!(adapter.contains("pub(crate) fn ensure_user_path("));
    assert!(adapter.contains("pub(crate) fn user_path_change_plan("));
    assert!(adapter.contains("pub(crate) fn validate_clean_targets("));
    assert!(adapter.contains("pub(crate) fn remove_clean_state("));
    assert!(uninstall_adapter.contains("pub(crate) fn user_path_removal_plan("));
    assert!(uninstall_adapter.contains("pub(crate) fn remove_user_path("));
    assert!(uninstall_adapter.contains("pub(crate) fn binary_removal_plan("));
    assert!(uninstall_adapter.contains("pub(crate) fn remove_installed_binary("));
    assert!(adapter.contains("#[path = \"system_install/tests.rs\"]"));
    assert!(adapter_tests.contains("fn clean_state_removes_only_managed_roots("));
    assert!(adapter_tests
        .contains("fn executable_install_creates_updates_and_preserves_managed_target("));
    assert!(adapter_tests.contains(
        "fn windows_powershell_path_update_is_idempotent_without_persisting_user_state("
    ));
    assert!(
        adapter_tests.contains("fn clean_uninstall_removes_binary_and_owned_profile_block_only(")
    );
    assert!(adapter_tests.contains("fn windows_powershell_path_removal_is_exact_and_idempotent("));

    let runtime_mutation =
        fs::read_to_string("src/adapters/filesystem/runtime_mutation.rs").unwrap();
    let generation =
        fs::read_to_string("src/app/inference_adapter/backend/generation_state.rs").unwrap();
    let backend_start =
        fs::read_to_string("src/app/inference_adapter/backend/sidecar/startup.rs").unwrap();
    assert!(runtime_mutation.contains("pub(crate) fn acquire("));
    assert!(generation.contains("runtime_mutation::acquire(\"backend generation begin\")"));
    assert!(backend_start.contains("runtime_mutation::acquire(\"backend start\")"));

    let dispatch = fs::read_to_string("src/app/command_dispatch.rs").unwrap();
    let lifecycle_dispatch =
        fs::read_to_string("src/app/command_dispatch/lifecycle_commands.rs").unwrap();
    assert!(dispatch.contains("Command::Install(command)"));
    assert!(dispatch.contains("Command::Init => execute_init()"));
    assert!(lifecycle_dispatch.contains("install::init_environment_report()?"));

    let help = fs::read_to_string("src/surfaces/cli/render.rs").unwrap();
    assert!(help.contains("rpotato install --clean --dry-run"));
    assert!(help.contains("rpotato install --clean --yes"));
    assert!(help.contains("rpotato uninstall --clean --dry-run"));
    assert!(help.contains("rpotato uninstall --clean --yes"));

    let release = fs::read_to_string(".github/workflows/release-binaries.yml").unwrap();
    let targeted = fs::read_to_string(".github/workflows/windows-native-targeted.yml").unwrap();
    for workflow in [release, targeted] {
        assert!(workflow.contains("adapters::system_install::tests::"));
    }
    let smoke = fs::read_to_string("scripts/release/verify-release-binary-smoke.sh").unwrap();
    assert!(smoke.contains("install --clean --dry-run"));
    let uninstall_smoke = fs::read_to_string("scripts/release/verify-uninstall-smoke.sh").unwrap();
    assert!(uninstall_smoke.contains("uninstall --clean --dry-run"));
}
