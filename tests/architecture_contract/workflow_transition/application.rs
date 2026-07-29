#[test]
fn v0376_workflow_application_owns_transaction_and_recovery_order() {
    let coordinator_tests =
        "src/runtime_core/workflow/application/transaction_coordinator/tests.rs";
    let ledger_adapter = "src/app/workflow_adapter/ledger.rs";
    let ledger_append = "src/app/workflow_adapter/ledger/append.rs";
    let ledger_derived = "src/app/workflow_adapter/ledger/derived.rs";
    let ledger_query = "src/app/workflow_adapter/ledger/query.rs";
    let ledger_storage = "src/app/workflow_adapter/ledger/storage.rs";
    let ledger_tests = "src/app/workflow_adapter/ledger/tests.rs";
    let ledger_writer = "src/app/workflow_adapter/ledger/writer.rs";
    let transition_adapter = "src/app/workflow_adapter/transition.rs";
    for target in [
        ledger_adapter,
        ledger_append,
        ledger_derived,
        ledger_query,
        ledger_storage,
        ledger_tests,
        ledger_writer,
        transition_adapter,
        "src/runtime_core/workflow/application/mod.rs",
        "src/runtime_core/workflow/application/recovery.rs",
        "src/runtime_core/workflow/application/transaction_coordinator.rs",
        coordinator_tests,
        "src/runtime_core/workflow/domain/transition.rs",
        "tests/workflow/recovery.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing workflow transaction/recovery owner: {target}"
        );
    }

    let workflow = fs::read_to_string("src/runtime_core/workflow/mod.rs").unwrap();
    assert!(
        workflow
            .lines()
            .any(|line| line == "pub(crate) mod application;"),
        "workflow application owner is not crate-private"
    );
    let application = fs::read_to_string("src/runtime_core/workflow/application/mod.rs").unwrap();
    for owner in ["recovery", "transaction_coordinator"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            application.lines().any(|line| line == expected),
            "workflow application owner is not crate-private: {owner}"
        );
    }

    let coordinator =
        fs::read_to_string("src/runtime_core/workflow/application/transaction_coordinator.rs")
            .unwrap();
    let coordinator_tests = fs::read_to_string(coordinator_tests).unwrap();
    assert!(
        coordinator.contains("#[path = \"transaction_coordinator/tests.rs\"]"),
        "transaction coordinator does not register its regression-test owner"
    );
    for rule in [
        "fn execute_approval_transaction",
        "fn execute_verification_transaction",
        "fn execute_terminal_action_transaction",
        "fn execute_state_transition",
        "fn execute_reconcile_transaction",
    ] {
        assert!(
            coordinator.contains(rule),
            "transaction coordinator is missing ordered use case: {rule}"
        );
    }
    for regression in [
        "fn accepts_only_the_next_bound_event(",
        "fn approval_commit_order_is_application_owned(",
        "fn verification_commit_and_recovery_share_one_order(",
        "fn reconcile_preserves_backup_before_canonical_append(",
    ] {
        assert!(
            coordinator_tests.contains(regression),
            "transaction coordinator regression owner is missing: {regression}"
        );
        assert!(
            !coordinator.contains(regression),
            "transaction coordinator still owns inline regression: {regression}"
        );
    }
    assert!(
        coordinator.lines().count() < 500,
        "transaction coordinator regrew beyond its ownership boundary"
    );
    assert!(
        coordinator_tests.lines().count() < 550,
        "transaction coordinator regression module regrew beyond its ownership boundary"
    );

    let recovery = fs::read_to_string("src/runtime_core/workflow/application/recovery.rs").unwrap();
    for rule in [
        "fn recover_workflow_transaction",
        "fn recover_prepared_state_transition",
    ] {
        assert!(
            recovery.contains(rule),
            "workflow recovery owner is missing policy: {rule}"
        );
    }

    for (facade, moved_definition) in [
        (ledger_adapter, "struct PlannedEvent"),
        (transition_adapter, "enum CurrentStateIntent"),
        (transition_adapter, "struct PreparedSourceBundle"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            !source.contains(moved_definition),
            "legacy facade still owns moved workflow definition: {facade} -> {moved_definition}"
        );
    }

    assert!(
        !Path::new("src/ledger.rs").exists(),
        "legacy workflow root was restored: src/ledger.rs"
    );
    assert!(
        !Path::new("src/transition.rs").exists(),
        "legacy workflow root was restored: src/transition.rs"
    );
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(
        !main.lines().any(|line| line == "mod ledger;"),
        "legacy workflow root remains registered: mod ledger;"
    );
    assert!(
        !main.lines().any(|line| line == "mod transition;"),
        "legacy workflow root remains registered: mod transition;"
    );
    let adapter_mod = fs::read_to_string("src/app/workflow_adapter.rs").unwrap();
    assert!(
        adapter_mod
            .lines()
            .any(|line| line == "pub(crate) mod ledger;"),
        "ledger adapter is not registered under workflow_adapter"
    );
    assert!(
        adapter_mod
            .lines()
            .any(|line| line == "pub(crate) mod transition;"),
        "transition adapter is not registered under workflow_adapter"
    );

    let ledger = fs::read_to_string(ledger_adapter).unwrap();
    let ledger_append = fs::read_to_string(ledger_append).unwrap();
    let ledger_derived_outputs = fs::read_to_string(ledger_derived).unwrap();
    let ledger_queries = fs::read_to_string(ledger_query).unwrap();
    let ledger_persistence = fs::read_to_string(ledger_storage).unwrap();
    let ledger_regressions = fs::read_to_string(ledger_tests).unwrap();
    let ledger_writes = fs::read_to_string(ledger_writer).unwrap();
    assert!(
        ledger.lines().any(|line| line == "mod append;"),
        "ledger adapter does not register its append owner"
    );
    assert!(
        ledger_append.contains("fn append_line") && ledger_append.contains("OpenOptions"),
        "ledger append primitive is missing durable append ownership"
    );
    assert!(
        ledger.lines().any(|line| line == "mod derived;"),
        "ledger adapter does not register its derived-output owner"
    );
    for responsibility in [
        "pub(super) fn converge_derived_outputs_unlocked(",
        "pub(super) fn validate_derived_outputs_unlocked(",
        "fn rebuild_operation_log_from_events(",
        "fn rebuild_project_ledger_from_events(",
        "pub(super) fn render_chained_ledger(",
    ] {
        assert!(
            ledger_derived_outputs.contains(responsibility),
            "ledger derived-output owner is missing: {responsibility}"
        );
        assert!(
            !ledger.contains(responsibility),
            "ledger adapter still owns derived-output behavior: {responsibility}"
        );
    }
    assert!(
        ledger.lines().any(|line| line == "mod query;"),
        "ledger adapter does not register its query owner"
    );
    for responsibility in [
        "pub fn event_detail_exists(",
        "pub fn event_details_match(",
        "pub fn workflow_checkpoint_exists(",
        "pub fn workflow_checkpoints(",
    ] {
        assert!(
            ledger_queries.contains(responsibility),
            "ledger query owner is missing: {responsibility}"
        );
        assert!(
            !ledger.contains(responsibility),
            "ledger adapter still owns query behavior: {responsibility}"
        );
    }
    assert!(
        ledger.lines().any(|line| line == "mod storage;"),
        "ledger adapter does not register its storage owner"
    );
    for responsibility in [
        "pub fn read_runtime_events(",
        "pub(crate) fn read_runtime_tail_read_only(",
        "pub(super) fn read_runtime_events_unlocked(",
        "pub(super) fn validate_ledger_contents(",
        "pub(super) fn append_chained_event(",
        "pub(super) fn write_ledger_head(",
        "fn validate_ledger_head(",
    ] {
        assert!(
            ledger_persistence.contains(responsibility),
            "ledger storage owner is missing: {responsibility}"
        );
        assert!(
            !ledger.contains(responsibility),
            "ledger adapter still owns storage behavior: {responsibility}"
        );
    }
    assert!(
        ledger.lines().any(|line| line == "mod writer;"),
        "ledger adapter does not register its writer owner"
    );
    for responsibility in [
        "pub(crate) struct LedgerWriterGuard",
        "pub(crate) struct EventSink<'guard>",
        "pub(crate) fn acquire()",
        "pub(crate) fn plan_events(",
        "pub(crate) fn append_runtime_planned(",
        "pub(crate) fn converge_prepared(",
        "fn validate_prepared_runtime_suffix(",
    ] {
        assert!(
            ledger_writes.contains(responsibility),
            "ledger writer owner is missing: {responsibility}"
        );
        assert!(
            !ledger.contains(responsibility),
            "ledger adapter still owns writer behavior: {responsibility}"
        );
    }
    assert!(
        ledger.contains("#[path = \"ledger/tests.rs\"]"),
        "ledger adapter does not register its regression-test owner"
    );
    for regression in [
        "fn physical_chain_reorder_and_truncation_fail_closed(",
        "fn concurrent_writers_preserve_both_ledger_chains(",
        "fn event_sink_single_acquisition_concurrency_matrix(",
        "fn t10_rebuilds_all_derived_outputs_from_runtime_authority(",
    ] {
        assert!(
            ledger_regressions.contains(regression),
            "ledger regression owner is missing: {regression}"
        );
        assert!(
            !ledger.contains(regression),
            "ledger adapter still owns regression test: {regression}"
        );
    }
    assert!(
        ledger.lines().count() < 225,
        "ledger adapter regrew beyond its test extraction boundary"
    );
    assert!(
        ledger_derived_outputs.lines().count() < 225,
        "ledger derived-output module regrew beyond its ownership boundary"
    );
    assert!(
        ledger_queries.lines().count() < 125,
        "ledger query module regrew beyond its ownership boundary"
    );
    assert!(
        ledger_persistence.lines().count() < 475,
        "ledger storage module regrew beyond its ownership boundary"
    );
    assert!(
        ledger_append.lines().count() < 75,
        "ledger append primitive regrew beyond its ownership boundary"
    );
    assert!(
        ledger_writes.lines().count() < 425,
        "ledger writer module regrew beyond its ownership boundary"
    );
    assert!(
        ledger_regressions.lines().count() < 575,
        "ledger regression module regrew beyond its ownership boundary"
    );

    let patch_loop = fs::read_to_string("tests/patch_loop.rs").unwrap();
    let patch_lifecycle = fs::read_to_string("tests/patch/lifecycle.rs").unwrap();
    assert!(
        patch_loop.contains("#[path = \"patch/lifecycle.rs\"]")
            && patch_lifecycle.contains("#[path = \"../workflow/recovery.rs\"]"),
        "patch-loop recovery filters are not owned by tests/workflow/recovery.rs"
    );
}
