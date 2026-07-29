use super::*;

#[test]
fn v03713_context_adapter_separates_filesystem_discovery() {
    let context_adapter = "src/app/context_adapter.rs";
    let context_compaction = "src/app/context_adapter/compaction.rs";
    let compaction_artifact_store = "src/app/context_adapter/compaction/artifact_store.rs";
    let filesystem_discovery = "src/app/context_adapter/discovery.rs";
    let context_tests = "src/app/context_adapter/tests.rs";
    assert!(Path::new(context_adapter).is_file());
    assert!(Path::new(context_compaction).is_file());
    assert!(Path::new(compaction_artifact_store).is_file());
    assert!(Path::new(filesystem_discovery).is_file());
    assert!(Path::new(context_tests).is_file());
    assert!(!Path::new("src/context.rs").exists());
    assert!(!Path::new("src/context").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod context;"));
    let app_root = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod context_adapter;"),
        "application root does not register the context adapter"
    );

    let context = fs::read_to_string(context_adapter).unwrap();
    let compaction = fs::read_to_string(context_compaction).unwrap();
    let artifact_store = fs::read_to_string(compaction_artifact_store).unwrap();
    let discovery = fs::read_to_string(filesystem_discovery).unwrap();
    let tests = fs::read_to_string(context_tests).unwrap();
    assert!(
        context.lines().any(|line| line == "mod discovery;"),
        "context adapter does not register its filesystem discovery owner"
    );
    assert!(
        context.lines().any(|line| line == "mod compaction;"),
        "context adapter does not register its compaction owner"
    );
    assert!(
        compaction.lines().any(|line| line == "mod artifact_store;"),
        "context compaction does not register its artifact-store owner"
    );
    for responsibility in [
        "pub(super) fn install_artifact(",
        "pub(crate) fn load_current_artifact(",
        "fn validate_artifact_chain(",
        "fn load_artifact_pointer(",
    ] {
        assert!(
            artifact_store.contains(responsibility),
            "compaction artifact-store owner is missing responsibility: {responsibility}"
        );
        assert!(
            !compaction.contains(responsibility),
            "compaction orchestration still owns artifact storage: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn build_filesystem_fallback(",
        "pub(super) fn discover_candidate_files(",
        "fn should_skip_dir(",
        "fn is_context_file(",
        "pub(super) fn request_terms(",
        "pub(super) fn score_path(",
        "pub(super) fn relative_path(",
        "pub(super) fn content_fingerprint(",
    ] {
        assert!(
            discovery.contains(responsibility),
            "filesystem discovery owner is missing responsibility: {responsibility}"
        );
        assert!(
            !context.contains(responsibility),
            "context orchestration still owns filesystem discovery: {responsibility}"
        );
    }
    assert!(
        tests.contains("fn filesystem_discovery_skips_generated_dirs_and_ranks_request_matches(")
    );
    assert!(context.lines().count() < 600);
    assert!(compaction.lines().count() < 550);
    assert!(artifact_store.lines().count() < 350);
    assert!(discovery.lines().count() < 250);
}

#[test]
fn v03713_evidence_adapter_separates_store_inspection() {
    let evidence_adapter = "src/app/evidence_adapter.rs";
    let evidence_store = "src/app/evidence_adapter/store.rs";
    assert!(Path::new(evidence_adapter).is_file());
    assert!(Path::new(evidence_store).is_file());
    assert!(!Path::new("src/evidence.rs").exists());
    assert!(!Path::new("src/evidence").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod evidence;"));
    let app_root = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod evidence_adapter;"),
        "application root does not register the evidence adapter"
    );

    let evidence = fs::read_to_string(evidence_adapter).unwrap();
    let store = fs::read_to_string(evidence_store).unwrap();
    assert!(
        evidence.lines().any(|line| line == "mod store;"),
        "evidence adapter does not register its store inspection owner"
    );
    for responsibility in [
        "pub fn store_status(",
        "pub(crate) fn store_status_bounded(",
        "fn count_jsonl_records(",
        "fn count_jsonl_records_bounded(",
        "fn count_top_level_files_bounded(",
        "fn count_files(",
    ] {
        assert!(
            store.contains(responsibility),
            "evidence store owner is missing responsibility: {responsibility}"
        );
        assert!(
            !evidence.contains(responsibility),
            "evidence orchestration still owns store inspection: {responsibility}"
        );
    }
    assert!(evidence
        .contains("fn bounded_store_status_reports_scan_truncation_and_rejects_zero_budget("));
    assert!(evidence.lines().count() < 600);
    assert!(store.lines().count() < 200);
}

#[test]
fn v03713_benchmark_adapter_separates_regression_tests() {
    let adapter_path = "src/app/inference_adapter/benchmark.rs";
    let tests_path = "src/app/inference_adapter/benchmark/tests.rs";
    assert!(Path::new(adapter_path).is_file());
    assert!(Path::new(tests_path).is_file());

    let adapter = fs::read_to_string(adapter_path).unwrap();
    let tests = fs::read_to_string(tests_path).unwrap();
    assert!(
        adapter.contains("#[path = \"benchmark/tests.rs\"]"),
        "benchmark adapter does not register its regression test owner"
    );
    for regression in [
        "fn validates_fixture_metadata(",
        "fn executable_run_records_local_score_without_prompt_text(",
        "fn rejects_raw_prompt_field(",
        "fn canonical_model_adoption_fixture_is_valid(",
    ] {
        assert!(
            tests.contains(regression),
            "benchmark test owner is missing regression: {regression}"
        );
        assert!(
            !adapter.contains(regression),
            "benchmark production adapter still owns regression: {regression}"
        );
    }
    assert!(adapter.lines().count() < 350);
    assert!(tests.lines().count() < 450);
}

#[test]
fn v03713_workflow_record_separates_compatibility_codec() {
    let record_path = "src/runtime_core/workflow/storage_compat/record.rs";
    let codec_path = "src/runtime_core/workflow/storage_compat/record/codec.rs";
    assert!(Path::new(record_path).is_file());
    assert!(Path::new(codec_path).is_file());

    let record = fs::read_to_string(record_path).unwrap();
    let codec = fs::read_to_string(codec_path).unwrap();
    assert!(record.contains("#[path = \"record/codec.rs\"]"));
    assert!(record.contains("pub struct WorkflowRecord"));
    assert!(record.contains("impl WorkflowRecord"));
    for responsibility in [
        "pub(crate) fn render_pointer(",
        "pub(crate) fn parse_pointer(",
        "pub(crate) fn snapshot_schema(",
        "pub(crate) fn parse_snapshot(",
        "pub(crate) fn payload(",
        "pub(crate) fn render(",
    ] {
        assert!(
            codec.contains(responsibility),
            "workflow record codec is missing responsibility: {responsibility}"
        );
        assert!(
            !record.contains(responsibility),
            "workflow record model still owns codec behavior: {responsibility}"
        );
    }
    assert!(codec.contains("const WORKFLOW_V2_KEYS"));
    assert!(codec.contains("const WORKFLOW_V3_KEYS"));
    assert!(codec.contains("const WORKFLOW_V4_KEYS"));
    assert!(record.lines().count() < 150);
    assert!(codec.lines().count() < 600);
}

#[test]
fn v03713_platform_fixtures_are_grouped_under_support_boundary() {
    for name in [
        "fake_sidecar.rs",
        "native_terminal.rs",
        "native_terminal_probe.rs",
    ] {
        assert!(!Path::new(&format!("tests/support/{name}")).exists());
        assert!(Path::new(&format!("tests/support/platform/{name}")).is_file());
    }

    let harness = fs::read_to_string("tests/surfaces.rs").unwrap();
    assert!(harness.contains("support/platform/native_terminal.rs"));
    assert!(harness.contains("surfaces/interactive_tui.rs"));
    assert!(harness.contains("surfaces/native_terminal.rs"));
    assert!(!Path::new("tests/platform.rs").exists());
    assert!(!Path::new("tests/platform").exists());

    let native_terminal = fs::read_to_string("tests/support/platform/native_terminal.rs").unwrap();
    let owners = [
        ("capture", 125),
        ("fixture", 450),
        ("process", 150),
        ("trace", 50),
        ("unix", 600),
        ("windows", 750),
    ];
    for (owner, line_budget) in owners {
        let relative = format!("native_terminal/{owner}.rs");
        assert!(
            native_terminal.contains(&relative),
            "native terminal facade does not register {owner}"
        );
        let source = fs::read_to_string(format!("tests/support/platform/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "native terminal owner {owner} exceeded its {line_budget}-line budget"
        );
    }
    assert!(native_terminal.lines().count() < 75);
}

#[test]
fn v03713_state_adapter_separates_persistence_responsibilities() {
    let atomic_write_adapter = "src/adapters/filesystem/atomic_write.rs";
    let state_adapter = "src/app/workflow_adapter/state.rs";
    let current_snapshot_adapter = "src/app/workflow_adapter/state/current_snapshot.rs";
    let current_snapshot_codec = "src/app/workflow_adapter/state/current_snapshot/codec.rs";
    let current_transition_adapter = "src/app/workflow_adapter/state/current_transition.rs";
    let current_image_adapter =
        "src/app/workflow_adapter/state/current_transition/current_image.rs";
    let lifecycle_adapter = "src/app/workflow_adapter/state/lifecycle.rs";
    let source_install_adapter = "src/app/workflow_adapter/state/source_install.rs";
    let source_install_directory = "src/app/workflow_adapter/state/source_install/directory.rs";
    let source_install_fd_ops = "src/app/workflow_adapter/state/source_install/fd_ops.rs";
    let transaction_adapter = "src/app/workflow_adapter/state/transaction.rs";
    let approval_transaction_adapter = "src/app/workflow_adapter/state/transaction/approval.rs";
    let terminal_transaction_adapter = "src/app/workflow_adapter/state/transaction/terminal.rs";
    let verification_transaction_adapter =
        "src/app/workflow_adapter/state/transaction/verification.rs";
    let transition_commit_adapter = "src/app/workflow_adapter/state/transition_commit.rs";
    let workflow_access_adapter = "src/app/workflow_adapter/state/workflow_access.rs";
    let workflow_revision_adapter = "src/app/workflow_adapter/state/workflow_revision.rs";
    let workflow_store_adapter = "src/app/workflow_adapter/state/workflow_store.rs";
    let state_test_modules = [
        "src/app/workflow_adapter/state/tests/mod.rs",
        "src/app/workflow_adapter/state/tests/callgraph.rs",
        "src/app/workflow_adapter/state/tests/current_snapshot.rs",
        "src/app/workflow_adapter/state/tests/lifecycle.rs",
        "src/app/workflow_adapter/state/tests/source_install.rs",
        "src/app/workflow_adapter/state/tests/workflow_store.rs",
    ];
    assert!(Path::new(atomic_write_adapter).is_file());
    assert!(Path::new(state_adapter).is_file());
    assert!(Path::new(current_snapshot_adapter).is_file());
    assert!(Path::new(current_snapshot_codec).is_file());
    assert!(Path::new(current_transition_adapter).is_file());
    assert!(Path::new(current_image_adapter).is_file());
    assert!(Path::new(lifecycle_adapter).is_file());
    assert!(Path::new(source_install_adapter).is_file());
    assert!(Path::new(source_install_directory).is_file());
    assert!(Path::new(source_install_fd_ops).is_file());
    assert!(Path::new(transaction_adapter).is_file());
    assert!(Path::new(approval_transaction_adapter).is_file());
    assert!(Path::new(terminal_transaction_adapter).is_file());
    assert!(Path::new(verification_transaction_adapter).is_file());
    assert!(Path::new(transition_commit_adapter).is_file());
    assert!(Path::new(workflow_access_adapter).is_file());
    assert!(Path::new(workflow_revision_adapter).is_file());
    assert!(Path::new(workflow_store_adapter).is_file());
    for test_module in state_test_modules {
        assert!(Path::new(test_module).is_file());
    }

    let state = fs::read_to_string(state_adapter).unwrap();
    assert!(state.lines().any(|line| line == "mod current_snapshot;"));
    assert!(state.lines().any(|line| line == "mod current_transition;"));
    assert!(state.lines().any(|line| line == "mod lifecycle;"));
    assert!(state.lines().any(|line| line == "mod source_install;"));
    assert!(state.lines().any(|line| line == "mod transaction;"));
    assert!(state.lines().any(|line| line == "mod transition_commit;"));
    assert!(state.lines().any(|line| line == "mod workflow_access;"));
    assert!(state.lines().any(|line| line == "mod workflow_revision;"));
    assert!(state.lines().any(|line| line == "mod workflow_store;"));
    assert!(state.contains("#[path = \"state/tests/mod.rs\"]"));
    assert!(!state.contains("mod tests {"));
    for escaped_responsibility in [
        "fn parse_current_state(",
        "fn promote_current_state_v1(",
        "struct StateTransitionRecoveryPort",
        "struct StateTransitionTransactionAdapter",
        "fn validate_prepared_state_current_member(",
        "struct StateReconcileTransactionPort",
        "fn reconcile_invalid_current_under_guard(",
        "fn decode_prepared_current_image(",
        "pub fn session_resume_report(",
        "pub fn reconcile_report(",
        "struct PreparedSourceDir",
        "fn recover_source_replace",
        "struct StateApprovalTransactionPort",
        "struct StateVerificationTransactionPort",
        "pub fn load_workflow(",
        "pub fn active_workflow_id(",
        "fn discover_active_workflow(",
        "pub(crate) fn clear_terminal_workflow_pointer(",
        "struct WorkflowCheckpointGuard",
        "fn build_prepared_workflow_revision(",
        "struct StateWorkflowRecoveryPort",
        "fn validate_workflow_chain(",
        "pub(crate) fn atomic_replace_bytes(",
    ] {
        assert!(
            !state.contains(escaped_responsibility),
            "state child responsibility escaped into parent adapter: {escaped_responsibility}"
        );
    }

    let atomic_write = fs::read_to_string(atomic_write_adapter).unwrap();
    for owned_responsibility in [
        "pub(crate) fn atomic_replace_bytes(",
        "pub(crate) fn replace_file(",
        "pub(crate) fn sync_parent(",
    ] {
        assert!(
            atomic_write.contains(owned_responsibility),
            "atomic write adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let current_snapshot = fs::read_to_string(current_snapshot_adapter).unwrap();
    assert!(current_snapshot.lines().any(|line| line == "mod codec;"));
    assert!(current_snapshot.contains("fn promote_current_state_v1("));
    for escaped_responsibility in ["fn parse_current_state(", "fn render_current_state_v2("] {
        assert!(
            !current_snapshot.contains(escaped_responsibility),
            "current snapshot codec responsibility escaped into orchestration: {escaped_responsibility}"
        );
    }
    let current_snapshot_codec = fs::read_to_string(current_snapshot_codec).unwrap();
    for owned_responsibility in [
        "fn parse_current_state(",
        "fn parse_current_state_v2(",
        "fn render_current_state_v2(",
    ] {
        assert!(
            current_snapshot_codec.contains(owned_responsibility),
            "current snapshot codec is missing responsibility: {owned_responsibility}"
        );
    }

    let current_transition = fs::read_to_string(current_transition_adapter).unwrap();
    for owned_responsibility in [
        "struct StateTransitionRecoveryPort",
        "struct StateTransitionTransactionAdapter",
    ] {
        assert!(
            current_transition.contains(owned_responsibility),
            "current transition adapter is missing responsibility: {owned_responsibility}"
        );
    }
    assert!(
        current_transition
            .lines()
            .any(|line| line == "mod current_image;"),
        "current transition adapter does not register its current-image owner"
    );
    let current_image = fs::read_to_string(current_image_adapter).unwrap();
    for owned_responsibility in [
        "pub(crate) fn prepare_current_image(",
        "pub(crate) fn prepare_current_image_after(",
        "pub(in super::super) fn prepare_state_transition_current_image(",
        "pub(in super::super) fn state_transition_current_member(",
        "pub(crate) fn validate_prepared_state_current_member(",
        "pub(in super::super) fn validate_state_transition_current_cas(",
    ] {
        assert!(
            current_image.contains(owned_responsibility),
            "current-image adapter is missing responsibility: {owned_responsibility}"
        );
        assert!(
            !current_transition.contains(owned_responsibility),
            "current transition orchestration still owns current-image policy: {owned_responsibility}"
        );
    }

    let lifecycle = fs::read_to_string(lifecycle_adapter).unwrap();
    for owned_responsibility in [
        "pub fn initialize(",
        "pub fn reconcile_report(",
        "pub fn session_resume_report(",
    ] {
        assert!(
            lifecycle.contains(owned_responsibility),
            "state lifecycle adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let source_install = fs::read_to_string(source_install_adapter).unwrap();
    let source_install_directory = fs::read_to_string(source_install_directory).unwrap();
    let source_install_fd_ops = fs::read_to_string(source_install_fd_ops).unwrap();
    assert!(
        source_install.lines().any(|line| line == "mod directory;"),
        "source installation adapter does not register its directory capability owner"
    );
    assert!(
        source_install.lines().any(|line| line == "mod fd_ops;"),
        "source installation adapter does not register its fd-relative I/O owner"
    );
    let source_install_responsibility = "fn recover_source_replace";
    assert!(
        source_install.contains(source_install_responsibility),
        "source installation adapter is missing responsibility: {source_install_responsibility}"
    );
    for owned_responsibility in [
        "pub(super) struct PreparedSourceDir",
        "pub(super) struct PreparedRollbackDir",
        "pub(super) fn validate_original(",
        "pub(super) fn validate_installed(",
        "pub(super) fn validate_original_pair(",
        "pub(super) fn validate_installed_pair(",
    ] {
        assert!(
            source_install_directory.contains(owned_responsibility),
            "source directory capability owner is missing responsibility: {owned_responsibility}"
        );
        assert!(
            !source_install.contains(owned_responsibility),
            "source installation transaction adapter still owns directory capability: {owned_responsibility}"
        );
    }
    for owned_responsibility in [
        "pub(super) mod unix_open_flags",
        "pub(super) fn openat_file(",
        "pub(super) fn mkdirat_directory(",
        "pub(super) fn dir_linkat(",
        "pub(super) fn dir_unlinkat(",
    ] {
        assert!(
            source_install_fd_ops.contains(owned_responsibility),
            "source fd-relative I/O owner is missing responsibility: {owned_responsibility}"
        );
        assert!(
            !source_install.contains(owned_responsibility),
            "source installation transaction adapter still owns fd-relative I/O: {owned_responsibility}"
        );
    }

    let workflow_store = fs::read_to_string(workflow_store_adapter).unwrap();
    for owned_responsibility in [
        "struct StateWorkflowRecoveryPort",
        "fn validate_workflow_chain(",
        "fn write_workflow_snapshot_bytes(",
    ] {
        assert!(
            workflow_store.contains(owned_responsibility),
            "workflow store adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let workflow_access = fs::read_to_string(workflow_access_adapter).unwrap();
    for owned_responsibility in [
        "pub fn load_workflow(",
        "pub(crate) fn load_workflow_revision(",
        "pub fn active_workflow_id(",
        "pub(crate) fn clear_terminal_workflow_pointer(",
        "pub(crate) fn record_tui_workflow_resume_receipt_under_transition(",
        "pub(crate) fn record_workflow_event_under_transition(",
        "pub(super) fn discover_active_workflow(",
    ] {
        assert!(
            workflow_access.contains(owned_responsibility),
            "workflow access adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let workflow_revision = fs::read_to_string(workflow_revision_adapter).unwrap();
    for owned_responsibility in [
        "struct WorkflowCheckpointGuard",
        "fn build_prepared_workflow_revision(",
        "fn decode_prepared_workflow_revision(",
    ] {
        assert!(
            workflow_revision.contains(owned_responsibility),
            "workflow revision adapter is missing responsibility: {owned_responsibility}"
        );
    }

    let transaction = fs::read_to_string(transaction_adapter).unwrap();
    let approval_transaction = fs::read_to_string(approval_transaction_adapter).unwrap();
    let terminal_transaction = fs::read_to_string(terminal_transaction_adapter).unwrap();
    let verification_transaction = fs::read_to_string(verification_transaction_adapter).unwrap();
    assert!(
        transaction.lines().any(|line| line == "mod approval;"),
        "state transaction adapter does not register its approval owner"
    );
    assert!(
        transaction.lines().any(|line| line == "mod terminal;"),
        "state transaction adapter does not register its terminal owner"
    );
    assert!(
        transaction.lines().any(|line| line == "mod verification;"),
        "state transaction adapter does not register its verification owner"
    );
    for owned_responsibility in [
        "pub(crate) struct PreparedApprovalTransition",
        "pub(crate) fn transition_project_current_state_prepared_approval(",
        "pub(crate) fn recover_project_current_state_prepared_approval(",
        "struct ApprovalProjectionRecoveryPort",
        "struct StateApprovalTransactionPort",
    ] {
        assert!(
            approval_transaction.contains(owned_responsibility),
            "approval transaction adapter is missing responsibility: {owned_responsibility}"
        );
        assert!(
            !transaction.contains(owned_responsibility),
            "state transaction adapter still owns approval behavior: {owned_responsibility}"
        );
    }
    for owned_responsibility in [
        "pub(crate) struct TerminalActionRequest",
        "pub(crate) fn transition_project_current_state_prepared_terminal_action(",
        "pub(crate) fn recover_project_current_state_prepared_terminal_action(",
        "struct StateTerminalActionTransactionPort",
        "fn terminal_action_fault(",
    ] {
        assert!(
            terminal_transaction.contains(owned_responsibility),
            "terminal transaction adapter is missing responsibility: {owned_responsibility}"
        );
        assert!(
            !transaction.contains(owned_responsibility),
            "state transaction adapter still owns terminal behavior: {owned_responsibility}"
        );
    }
    for owned_responsibility in [
        "pub(crate) struct PreparedVerificationTransition",
        "pub(crate) fn transition_project_current_state_prepared_verification(",
        "pub(crate) fn recover_project_current_state_prepared_verification(",
        "struct StateVerificationTransactionPort",
    ] {
        assert!(
            verification_transaction.contains(owned_responsibility),
            "verification transaction adapter is missing responsibility: {owned_responsibility}"
        );
        assert!(
            !transaction.contains(owned_responsibility),
            "state transaction facade still owns verification behavior: {owned_responsibility}"
        );
    }

    let transition_commit = fs::read_to_string(transition_commit_adapter).unwrap();
    for owned_responsibility in [
        "struct StateReconcileTransactionPort",
        "fn reconcile_invalid_current_under_guard(",
        "fn decode_prepared_current_image(",
    ] {
        assert!(
            transition_commit.contains(owned_responsibility),
            "state transition commit adapter is missing responsibility: {owned_responsibility}"
        );
    }

    assert!(state.lines().count() < 450);
    assert!(current_snapshot.lines().count() < 700);
    assert!(current_snapshot_codec.lines().count() < 450);
    assert!(current_transition.lines().count() < 400);
    assert!(current_image.lines().count() < 325);
    assert!(lifecycle.lines().count() < 700);
    assert!(source_install.lines().count() < 375);
    assert!(source_install_directory.lines().count() < 375);
    assert!(source_install_fd_ops.lines().count() < 175);
    assert!(transaction.lines().count() < 30);
    assert!(approval_transaction.lines().count() < 250);
    assert!(terminal_transaction.lines().count() < 325);
    assert!(verification_transaction.lines().count() < 150);
    assert!(transition_commit.lines().count() < 450);
    assert!(workflow_access.lines().count() < 400);
    assert!(workflow_revision.lines().count() < 500);
    assert!(workflow_store.lines().count() < 500);
    for test_module in state_test_modules {
        let tests = fs::read_to_string(test_module).unwrap();
        assert!(
            tests.lines().count() < 700,
            "oversized state test module: {test_module}"
        );
    }
}
