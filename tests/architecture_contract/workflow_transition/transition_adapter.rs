#[test]
fn v03713_transition_adapter_delegates_source_install_contract() {
    let transition_adapter = "src/app/workflow_adapter/transition.rs";
    let bundle_codec_adapter = "src/app/workflow_adapter/transition/bundle_codec.rs";
    let bundle_additional_members_adapter =
        "src/app/workflow_adapter/transition/bundle_codec/additional_members.rs";
    let bundle_semantic_adapter = "src/app/workflow_adapter/transition/bundle_codec/semantic.rs";
    let bundle_source_members_adapter =
        "src/app/workflow_adapter/transition/bundle_codec/source_members.rs";
    let bundle_preparation_adapter = "src/app/workflow_adapter/transition/bundle_preparation.rs";
    let bundle_construction_adapter =
        "src/app/workflow_adapter/transition/bundle_preparation/construction.rs";
    let bundle_event_plan_adapter =
        "src/app/workflow_adapter/transition/bundle_preparation/event_plan.rs";
    let bundle_members_preparation_adapter =
        "src/app/workflow_adapter/transition/bundle_preparation/members.rs";
    let bundle_projection_lag_adapter =
        "src/app/workflow_adapter/transition/bundle_preparation/projection_lag.rs";
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
    let source_install_codec_adapter =
        "src/app/workflow_adapter/transition/source_install/codec.rs";
    let source_install_paths_adapter =
        "src/app/workflow_adapter/transition/source_install/paths.rs";
    let source_install_preparation_adapter =
        "src/app/workflow_adapter/transition/source_install/preparation.rs";
    let source_install_validation_adapter =
        "src/app/workflow_adapter/transition/source_install/validation.rs";
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
        bundle_additional_members_adapter,
        bundle_semantic_adapter,
        bundle_source_members_adapter,
        bundle_preparation_adapter,
        bundle_construction_adapter,
        bundle_event_plan_adapter,
        bundle_members_preparation_adapter,
        bundle_projection_lag_adapter,
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
        source_install_codec_adapter,
        source_install_paths_adapter,
        source_install_preparation_adapter,
        source_install_validation_adapter,
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
    let bundle_additional_members = fs::read_to_string(bundle_additional_members_adapter).unwrap();
    let bundle_semantic = fs::read_to_string(bundle_semantic_adapter).unwrap();
    let bundle_source_members = fs::read_to_string(bundle_source_members_adapter).unwrap();
    let bundle_preparation = fs::read_to_string(bundle_preparation_adapter).unwrap();
    let bundle_construction = fs::read_to_string(bundle_construction_adapter).unwrap();
    let bundle_event_plan = fs::read_to_string(bundle_event_plan_adapter).unwrap();
    let bundle_members_preparation =
        fs::read_to_string(bundle_members_preparation_adapter).unwrap();
    let bundle_projection_lag = fs::read_to_string(bundle_projection_lag_adapter).unwrap();
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
    let source_install_codec = fs::read_to_string(source_install_codec_adapter).unwrap();
    let source_install_paths = fs::read_to_string(source_install_paths_adapter).unwrap();
    let source_install_preparation =
        fs::read_to_string(source_install_preparation_adapter).unwrap();
    let source_install_validation = fs::read_to_string(source_install_validation_adapter).unwrap();
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
    include!("transition_adapter/component_delegation.rs");
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
    for owner in ["codec", "paths", "preparation", "validation"] {
        assert!(
            source_install
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "source-install facade does not register {owner}"
        );
    }
    for (owner, responsibilities) in [
        (
            source_install_preparation.as_str(),
            &["pub(crate) fn prepare_source_install_v1("][..],
        ),
        (
            source_install_validation.as_str(),
            &["pub(crate) fn validate_source_install_v1("][..],
        ),
        (
            source_install_codec.as_str(),
            &[
                "pub(crate) fn render_source_install_v1(",
                "pub(crate) fn parse_source_install_v1(",
            ][..],
        ),
        (
            source_install_paths.as_str(),
            &[
                "pub(crate) fn source_identity_v1(",
                "pub(crate) fn resolve_prepared_project_path(",
                "pub(crate) fn source_install_rollback_path(",
            ][..],
        ),
    ] {
        for responsibility in responsibilities {
            assert!(
                !transition.contains(responsibility),
                "source-install responsibility escaped into transition facade: {responsibility}"
            );
            assert!(
                !source_install.contains(responsibility),
                "source-install facade still owns responsibility: {responsibility}"
            );
            assert!(
                owner.contains(responsibility),
                "source-install child is missing responsibility: {responsibility}"
            );
        }
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
        (bundle_codec_adapter, bundle_codec.as_str(), 30),
        (
            bundle_additional_members_adapter,
            bundle_additional_members.as_str(),
            175,
        ),
        (bundle_semantic_adapter, bundle_semantic.as_str(), 175),
        (
            bundle_source_members_adapter,
            bundle_source_members.as_str(),
            275,
        ),
        (bundle_preparation_adapter, bundle_preparation.as_str(), 30),
        (
            bundle_construction_adapter,
            bundle_construction.as_str(),
            250,
        ),
        (bundle_event_plan_adapter, bundle_event_plan.as_str(), 75),
        (
            bundle_members_preparation_adapter,
            bundle_members_preparation.as_str(),
            50,
        ),
        (
            bundle_projection_lag_adapter,
            bundle_projection_lag.as_str(),
            225,
        ),
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
        (source_install_adapter, source_install.as_str(), 25),
        (
            source_install_codec_adapter,
            source_install_codec.as_str(),
            125,
        ),
        (
            source_install_paths_adapter,
            source_install_paths.as_str(),
            75,
        ),
        (
            source_install_preparation_adapter,
            source_install_preparation.as_str(),
            275,
        ),
        (
            source_install_validation_adapter,
            source_install_validation.as_str(),
            125,
        ),
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
