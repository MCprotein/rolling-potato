use super::*;

#[test]
fn team_reconciliation_has_bounded_coordinator_and_handlers() {
    let coordinator_path = "src/app/collaboration_adapter/team_reconciliation.rs";
    let coordinator = source(coordinator_path);
    assert_line_bound(&coordinator, 200, coordinator_path);
    for (owner, limit, responsibilities) in [
        (
            "artifact",
            75,
            &["pub(super) fn render(", "pub(super) fn install("][..],
        ),
        (
            "evidence",
            150,
            &[
                "pub(super) fn verify_member_inputs(",
                "pub(super) fn merge_parent(",
                "pub(super) fn verify_stop_gate(",
            ][..],
        ),
        (
            "events",
            100,
            &[
                "pub(super) fn append_once(",
                "pub(super) fn has(",
                "pub(super) fn stop_gate_error(",
            ][..],
        ),
        (
            "members",
            175,
            &[
                "struct ReconciledMember",
                "pub(super) fn collect(",
                "fn admitted_bindings(",
                "fn reconcile_member(",
            ][..],
        ),
    ] {
        assert_registered(
            &coordinator,
            &format!("mod {owner};"),
            "team reconciliation coordinator",
        );
        let path = format!("src/app/collaboration_adapter/team_reconciliation/{owner}.rs");
        let owner_source = source(&path);
        assert_line_bound(&owner_source, limit, &path);
        for responsibility in responsibilities {
            assert_moved(&owner_source, &coordinator, responsibility);
        }
    }
    for moved_definition in ["fn validate_team_binding", "fn validate_member_record"] {
        assert!(
            !coordinator.contains(moved_definition),
            "team reconciliation adapter retains domain rule: {moved_definition}"
        );
    }
    for delegation in ["validate_reconciliation_binding", "validate_member_record"] {
        let handlers = format!(
            "{}{}",
            coordinator,
            source("src/app/collaboration_adapter/team_reconciliation/members.rs")
        );
        assert!(
            handlers.contains(delegation),
            "team reconciliation adapter is missing domain delegation: {delegation}"
        );
    }
}
