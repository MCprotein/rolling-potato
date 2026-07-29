{
    assert!(
        transition.lines().any(|line| line == "mod bundle_codec;"),
        "transition adapter does not register the bundle-codec owner"
    );
    for owner in ["additional_members", "semantic", "source_members"] {
        assert!(
            bundle_codec
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "bundle-codec facade does not register {owner}"
        );
    }
    for (owner, responsibilities) in [
        (
            bundle_source_members.as_str(),
            &["fn render_source_members(", "fn parse_source_members("][..],
        ),
        (
            bundle_additional_members.as_str(),
            &[
                "struct PreparedMemberParseContext",
                "fn parse_additional_members(",
            ][..],
        ),
        (
            bundle_semantic.as_str(),
            &[
                "fn parse_semantic_events(",
                "fn parse_event_chain_plan(",
                "fn prepared_member_order(",
            ][..],
        ),
    ] {
        for responsibility in responsibilities {
            assert!(
                !transition.contains(responsibility),
                "bundle-codec responsibility escaped into transition facade: {responsibility}"
            );
            assert!(
                !bundle_codec.contains(responsibility),
                "bundle-codec facade still owns responsibility: {responsibility}"
            );
            assert!(
                owner.contains(responsibility),
                "bundle-codec child is missing responsibility: {responsibility}"
            );
        }
    }
    assert!(
        transition
            .lines()
            .any(|line| line == "mod bundle_preparation;"),
        "transition adapter does not register the bundle-preparation owner"
    );
    for module in [
        "mod construction;",
        "mod event_plan;",
        "mod members;",
        "mod projection_lag;",
    ] {
        assert!(
            bundle_preparation.lines().any(|line| line == module),
            "bundle-preparation facade does not register {module}"
        );
    }
    for (owner, responsibilities) in [
        (
            bundle_construction.as_str(),
            &[
                "pub(crate) fn prepare_state_transition_bundle(",
                "pub(crate) fn prepare_source_bundle_with_context(",
            ][..],
        ),
        (
            bundle_event_plan.as_str(),
            &[
                "pub(crate) fn planned_events(",
                "pub(crate) fn bind_planned_events(",
            ][..],
        ),
        (
            bundle_members_preparation.as_str(),
            &["pub(crate) fn bind_additional_members("][..],
        ),
        (
            bundle_projection_lag.as_str(),
            &[
                "pub(crate) fn prepare_projection_lag_member(",
                "pub(crate) fn install_projection_lag(",
                "pub(crate) fn remove_projection_lag(",
            ][..],
        ),
    ] {
        for responsibility in responsibilities {
            assert!(
                !transition.contains(responsibility),
                "bundle-preparation responsibility escaped into transition facade: {responsibility}"
            );
            assert!(
                !bundle_preparation.contains(responsibility),
                "bundle-preparation facade still owns responsibility: {responsibility}"
            );
            assert!(
                owner.contains(responsibility),
                "bundle-preparation child is missing responsibility: {responsibility}"
            );
        }
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
}
