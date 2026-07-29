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
    let canonical_adapter = "src/app/workflow_adapter/transition/canonical.rs";
    let contracts_adapter = "src/app/workflow_adapter/transition/contracts.rs";
    let journal_adapter = "src/app/workflow_adapter/transition/journal.rs";
    let journal_codec_adapter = "src/app/workflow_adapter/transition/journal/codec.rs";
    let journal_guard_adapter = "src/app/workflow_adapter/transition/journal/guard.rs";
    let journal_persistence_adapter = "src/app/workflow_adapter/transition/journal/persistence.rs";
    let journal_recovery_adapter = "src/app/workflow_adapter/transition/journal/recovery.rs";
    let journal_recovery_io_adapter = "src/app/workflow_adapter/transition/journal/recovery_io.rs";
    let source_install_adapter = "src/app/workflow_adapter/transition/source_install.rs";
    let source_support_adapter = "src/app/workflow_adapter/transition/source_support.rs";
    let transition_tests = "src/app/workflow_adapter/transition/tests/mod.rs";
    let transition_recovery_tests =
        "src/app/workflow_adapter/transition/tests/recovery_and_contracts.rs";
    let transition_source_install_tests =
        "src/app/workflow_adapter/transition/tests/source_install.rs";
    let transition_prepared_bundle_tests =
        "src/app/workflow_adapter/transition/tests/prepared_bundle.rs";
    for target in [
        transition_adapter,
        bundle_codec_adapter,
        bundle_preparation_adapter,
        bundle_validation_adapter,
        bundle_event_chain_adapter,
        bundle_members_adapter,
        bundle_workflow_members_adapter,
        canonical_adapter,
        contracts_adapter,
        journal_adapter,
        journal_codec_adapter,
        journal_guard_adapter,
        journal_persistence_adapter,
        journal_recovery_adapter,
        journal_recovery_io_adapter,
        source_install_adapter,
        source_support_adapter,
        transition_tests,
        transition_recovery_tests,
        transition_source_install_tests,
        transition_prepared_bundle_tests,
    ] {
        assert!(Path::new(target).is_file());
    }

    let transition = fs::read_to_string(transition_adapter).unwrap();
    let bundle_codec = fs::read_to_string(bundle_codec_adapter).unwrap();
    let bundle_preparation = fs::read_to_string(bundle_preparation_adapter).unwrap();
    let bundle_validation = fs::read_to_string(bundle_validation_adapter).unwrap();
    let bundle_event_chain = fs::read_to_string(bundle_event_chain_adapter).unwrap();
    let bundle_members = fs::read_to_string(bundle_members_adapter).unwrap();
    let bundle_workflow_members = fs::read_to_string(bundle_workflow_members_adapter).unwrap();
    let canonical = fs::read_to_string(canonical_adapter).unwrap();
    let contracts = fs::read_to_string(contracts_adapter).unwrap();
    let journal = fs::read_to_string(journal_adapter).unwrap();
    let journal_codec = fs::read_to_string(journal_codec_adapter).unwrap();
    let journal_guard = fs::read_to_string(journal_guard_adapter).unwrap();
    let journal_persistence = fs::read_to_string(journal_persistence_adapter).unwrap();
    let journal_recovery = fs::read_to_string(journal_recovery_adapter).unwrap();
    let journal_recovery_io = fs::read_to_string(journal_recovery_io_adapter).unwrap();
    let source_install = fs::read_to_string(source_install_adapter).unwrap();
    let source_support = fs::read_to_string(source_support_adapter).unwrap();
    let test_owner = fs::read_to_string(transition_tests).unwrap();
    let recovery_tests = fs::read_to_string(transition_recovery_tests).unwrap();
    let source_install_tests = fs::read_to_string(transition_source_install_tests).unwrap();
    let prepared_bundle_tests = fs::read_to_string(transition_prepared_bundle_tests).unwrap();
    let tests = format!("{recovery_tests}{source_install_tests}{prepared_bundle_tests}");
    for (module, owner, responsibilities) in [
        (
            "mod canonical;",
            canonical.as_str(),
            &["fn required_object<'a>(", "fn render_path("][..],
        ),
        (
            "mod contracts;",
            contracts.as_str(),
            &["fn enforce_byte_limit(", "const PREPARED_BUNDLE_KEYS"][..],
        ),
        (
            "mod source_support;",
            source_support.as_str(),
            &["fn validate_stored_path(", "fn sha256_bytes("][..],
        ),
    ] {
        assert_registered_owner(&transition, module, owner, responsibilities);
    }
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
    assert!(journal.lines().any(|line| line == "mod guard;"));
    assert!(journal.lines().any(|line| line == "mod persistence;"));
    assert!(journal.lines().any(|line| line == "mod recovery;"));
    assert!(journal.lines().any(|line| line == "mod recovery_io;"));
    for responsibility in [
        "pub(crate) struct TransitionGuard",
        "pub(crate) fn commit_prepared_source_bundle(",
        "pub(crate) fn recover_pending_source_bundles(",
    ] {
        assert!(
            !transition.contains(responsibility),
            "journal responsibility escaped into transition facade: {responsibility}"
        );
        assert!(
            !journal.contains(responsibility),
            "transition journal facade still owns responsibility: {responsibility}"
        );
    }
    for (owner, responsibility) in [
        (&journal_guard, "pub(crate) struct TransitionGuard"),
        (
            &journal_persistence,
            "pub(crate) fn commit_prepared_source_bundle(",
        ),
        (
            &journal_persistence,
            "pub(crate) fn remove_committed_source_bundle(",
        ),
        (
            &journal_recovery,
            "pub(crate) fn recover_pending_source_bundles(",
        ),
        (&journal_recovery, "fn recover_pending_bundles_under_guard("),
    ] {
        assert!(
            owner.contains(responsibility),
            "transition journal owner is missing responsibility: {responsibility}"
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
    for test_file in [
        "recovery_and_contracts.rs",
        "source_install.rs",
        "prepared_bundle.rs",
    ] {
        assert!(
            test_owner.contains(&format!("include!(\"{test_file}\");")),
            "transition regression test owner does not include {test_file}"
        );
    }
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
    for (path, contents, maximum_lines) in [
        (transition_adapter, transition.as_str(), 125),
        (bundle_codec_adapter, bundle_codec.as_str(), 550),
        (bundle_preparation_adapter, bundle_preparation.as_str(), 500),
        (bundle_validation_adapter, bundle_validation.as_str(), 125),
        (bundle_event_chain_adapter, bundle_event_chain.as_str(), 100),
        (bundle_members_adapter, bundle_members.as_str(), 275),
        (
            bundle_workflow_members_adapter,
            bundle_workflow_members.as_str(),
            350,
        ),
        (canonical_adapter, canonical.as_str(), 500),
        (contracts_adapter, contracts.as_str(), 500),
        (journal_adapter, journal.as_str(), 75),
        (journal_codec_adapter, journal_codec.as_str(), 250),
        (journal_guard_adapter, journal_guard.as_str(), 100),
        (
            journal_persistence_adapter,
            journal_persistence.as_str(),
            300,
        ),
        (journal_recovery_adapter, journal_recovery.as_str(), 300),
        (
            journal_recovery_io_adapter,
            journal_recovery_io.as_str(),
            225,
        ),
        (source_install_adapter, source_install.as_str(), 500),
        (source_support_adapter, source_support.as_str(), 500),
    ] {
        assert!(
            contents.lines().count() < maximum_lines,
            "transition owner regrew beyond its boundary: {path}"
        );
    }
    assert!(test_owner.lines().count() < 20);
    for (path, contents) in [
        (transition_recovery_tests, recovery_tests),
        (transition_source_install_tests, source_install_tests),
        (transition_prepared_bundle_tests, prepared_bundle_tests),
    ] {
        assert!(contents.lines().count() < 500, "{path}");
    }
}

fn assert_registered_owner(facade: &str, module: &str, owner: &str, responsibilities: &[&str]) {
    assert!(facade.lines().any(|line| line == module));
    for responsibility in responsibilities {
        assert!(owner.contains(responsibility));
        assert!(!facade.contains(responsibility));
    }
}
