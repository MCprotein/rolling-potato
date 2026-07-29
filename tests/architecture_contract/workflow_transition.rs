use super::*;

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

#[test]
fn v03713_transition_adapter_delegates_source_install_contract() {
    let transition_adapter = "src/app/workflow_adapter/transition.rs";
    let bundle_codec_adapter = "src/app/workflow_adapter/transition/bundle_codec.rs";
    let bundle_preparation_adapter = "src/app/workflow_adapter/transition/bundle_preparation.rs";
    let bundle_validation_adapter = "src/app/workflow_adapter/transition/bundle_validation.rs";
    let bundle_event_chain_adapter =
        "src/app/workflow_adapter/transition/bundle_validation/event_chain.rs";
    let bundle_members_adapter = "src/app/workflow_adapter/transition/bundle_validation/members.rs";
    let bundle_workflow_members_adapter =
        "src/app/workflow_adapter/transition/bundle_validation/members/workflow.rs";
    let journal_adapter = "src/app/workflow_adapter/transition/journal.rs";
    let journal_codec_adapter = "src/app/workflow_adapter/transition/journal/codec.rs";
    let journal_recovery_io_adapter = "src/app/workflow_adapter/transition/journal/recovery_io.rs";
    let source_install_adapter = "src/app/workflow_adapter/transition/source_install.rs";
    let transition_tests = "src/app/workflow_adapter/transition/tests/mod.rs";
    for target in [
        transition_adapter,
        bundle_codec_adapter,
        bundle_preparation_adapter,
        bundle_validation_adapter,
        bundle_event_chain_adapter,
        bundle_members_adapter,
        bundle_workflow_members_adapter,
        journal_adapter,
        journal_codec_adapter,
        journal_recovery_io_adapter,
        source_install_adapter,
        transition_tests,
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing transition adapter owner: {target}"
        );
    }

    let transition = fs::read_to_string(transition_adapter).unwrap();
    let bundle_codec = fs::read_to_string(bundle_codec_adapter).unwrap();
    let bundle_preparation = fs::read_to_string(bundle_preparation_adapter).unwrap();
    let bundle_validation = fs::read_to_string(bundle_validation_adapter).unwrap();
    let bundle_event_chain = fs::read_to_string(bundle_event_chain_adapter).unwrap();
    let bundle_members = fs::read_to_string(bundle_members_adapter).unwrap();
    let bundle_workflow_members = fs::read_to_string(bundle_workflow_members_adapter).unwrap();
    let journal = fs::read_to_string(journal_adapter).unwrap();
    let journal_codec = fs::read_to_string(journal_codec_adapter).unwrap();
    let journal_recovery_io = fs::read_to_string(journal_recovery_io_adapter).unwrap();
    let source_install = fs::read_to_string(source_install_adapter).unwrap();
    let tests = fs::read_to_string(transition_tests).unwrap();
    assert!(
        transition.lines().any(|line| line == "mod bundle_codec;"),
        "transition adapter does not register the bundle-codec owner"
    );
    for responsibility in [
        "pub(super) fn render_source_members(",
        "pub(super) fn parse_source_members(",
        "pub(super) struct PreparedMemberParseContext",
        "pub(super) fn parse_semantic_events(",
        "pub(super) fn parse_event_chain_plan(",
        "pub(super) fn prepared_member_order(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "bundle-codec responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            bundle_codec.contains(responsibility),
            "bundle-codec adapter is missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition
            .lines()
            .any(|line| line == "mod bundle_preparation;"),
        "transition adapter does not register the bundle-preparation owner"
    );
    for responsibility in [
        "pub(crate) fn prepare_state_transition_bundle(",
        "pub(crate) fn prepare_source_bundle_with_context(",
        "pub(crate) fn prepare_projection_lag_member(",
        "pub(crate) fn install_projection_lag(",
        "pub(crate) fn bind_planned_events(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "bundle-preparation responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            bundle_preparation.contains(responsibility),
            "bundle-preparation adapter is missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition
            .lines()
            .any(|line| line == "mod bundle_validation;"),
        "transition adapter does not register the bundle-validation owner"
    );
    assert!(
        bundle_validation.contains("pub(super) fn validate_prepared_source_bundle("),
        "bundle-validation adapter is missing top-level bundle validation"
    );
    assert!(
        bundle_validation.lines().any(|line| line == "mod members;"),
        "bundle-validation adapter does not register its member validation owner"
    );
    for responsibility in [
        "pub(super) fn validate_additional_members(",
        "fn validate_projection_lag_member(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "bundle-validation responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            bundle_members.contains(responsibility),
            "bundle member-validation adapter is missing responsibility: {responsibility}"
        );
        assert!(
            !bundle_validation.contains(responsibility),
            "bundle-validation adapter still owns member validation: {responsibility}"
        );
    }
    assert!(
        bundle_members.lines().any(|line| line == "mod workflow;"),
        "bundle member-validation adapter does not register its workflow member owner"
    );
    for responsibility in [
        "pub(super) fn validate_state_transition_members(",
        "pub(super) fn validate_verification_members(",
    ] {
        assert!(
            bundle_workflow_members.contains(responsibility),
            "workflow member-validation adapter is missing responsibility: {responsibility}"
        );
        assert!(
            !bundle_members.contains(responsibility),
            "bundle member-validation adapter still owns workflow validation: {responsibility}"
        );
        assert!(
            !transition.contains(responsibility) && !bundle_validation.contains(responsibility),
            "workflow member validation escaped into an orchestration facade: {responsibility}"
        );
    }
    assert!(
        bundle_validation
            .lines()
            .any(|line| line == "mod event_chain;"),
        "bundle-validation adapter does not register its event-chain owner"
    );
    assert!(
        bundle_event_chain.contains("pub(in super::super) fn validate_event_chain("),
        "bundle event-chain owner is missing validation responsibility"
    );
    assert!(
        !bundle_validation.contains("fn validate_event_chain("),
        "bundle-validation adapter still owns event-chain validation"
    );
    assert!(
        transition.lines().any(|line| line == "mod journal;"),
        "transition adapter does not register the journal owner"
    );
    assert!(journal.lines().any(|line| line == "mod codec;"));
    assert!(journal.lines().any(|line| line == "mod recovery_io;"));
    for responsibility in [
        "pub(crate) struct TransitionGuard",
        "pub(crate) fn commit_prepared_source_bundle(",
        "pub(crate) fn recover_pending_source_bundles(",
        "fn recover_pending_bundles_under_guard(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "journal responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            journal.contains(responsibility),
            "transition journal adapter is missing responsibility: {responsibility}"
        );
    }
    for responsibility in [
        "pub(crate) fn render_prepared_source_bundle(",
        "pub(crate) fn parse_prepared_source_bundle(",
    ] {
        assert!(
            journal_codec.contains(responsibility),
            "transition journal codec is missing responsibility: {responsibility}"
        );
        assert!(
            !journal.contains(responsibility),
            "transition journal orchestration still owns codec behavior: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn bounded_regular_entries(",
        "fn read_regular_utf8_bounded(",
        "fn validate_open_regular_file_identity(",
        "fn recovery_work_may_exist(",
        "fn directory_has_entry_or_is_suspicious(",
    ] {
        assert!(
            journal_recovery_io.contains(responsibility),
            "transition recovery I/O adapter is missing responsibility: {responsibility}"
        );
        assert!(
            !journal.contains(responsibility),
            "transition journal orchestration still owns recovery I/O: {responsibility}"
        );
    }
    assert!(
        transition.lines().any(|line| line == "mod source_install;"),
        "transition adapter does not register the source-install owner"
    );
    assert!(
        transition
            .lines()
            .any(|line| line == "pub(crate) use source_install::{"),
        "transition adapter does not expose the source-install contract"
    );
    for responsibility in [
        "pub(crate) fn prepare_source_install_v1(",
        "pub(crate) fn validate_source_install_v1(",
        "pub(crate) fn render_source_install_v1(",
        "pub(crate) fn parse_source_install_v1(",
        "pub(crate) fn source_identity_v1(",
        "pub(crate) fn resolve_prepared_project_path(",
        "pub(crate) fn source_install_rollback_path(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "source-install responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            source_install.contains(responsibility),
            "source-install adapter is missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition.contains("#[path = \"transition/tests/mod.rs\"]"),
        "transition adapter does not register its regression test owner"
    );
    for responsibility in [
        "fn recovery_enforces_file_and_directory_read_bounds_before_parsing(",
        "fn source_install_v1_round_trips_exact_order_and_bindings(",
        "fn prepared_bundle_strictly_binds_semantic_event_chain_plan(",
    ] {
        assert!(
            tests.contains(responsibility),
            "transition regression tests are missing responsibility: {responsibility}"
        );
    }
    assert!(
        transition.lines().count() < 625,
        "transition adapter regrew beyond its extracted ownership boundary"
    );
    assert!(
        bundle_codec.lines().count() < 550,
        "bundle-codec adapter regrew beyond its ownership boundary"
    );
    assert!(
        bundle_preparation.lines().count() < 500,
        "bundle-preparation adapter regrew beyond its ownership boundary"
    );
    assert!(
        bundle_validation.lines().count() < 125,
        "bundle-validation adapter regrew beyond its ownership boundary"
    );
    assert!(
        bundle_event_chain.lines().count() < 100,
        "bundle event-chain adapter regrew beyond its ownership boundary"
    );
    assert!(
        bundle_members.lines().count() < 275,
        "bundle member-validation adapter regrew beyond its ownership boundary"
    );
    assert!(
        bundle_workflow_members.lines().count() < 350,
        "workflow member-validation adapter regrew beyond its ownership boundary"
    );
    assert!(
        journal.lines().count() < 550,
        "transition journal adapter regrew beyond its ownership boundary"
    );
    assert!(
        journal_codec.lines().count() < 250,
        "transition journal codec regrew beyond its ownership boundary"
    );
    assert!(
        journal_recovery_io.lines().count() < 225,
        "transition recovery I/O adapter regrew beyond its ownership boundary"
    );
    assert!(
        source_install.lines().count() < 500,
        "source-install adapter regrew beyond its ownership boundary"
    );
    assert!(
        tests.lines().count() < 750,
        "transition regression tests regrew beyond their ownership boundary"
    );
}
