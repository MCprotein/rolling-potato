#[allow(clippy::too_many_arguments)]
fn assert_transaction_and_recovery_contract(
    coordinator_path: &str,
    coordinator_approval_path: &str,
    coordinator_contracts_path: &str,
    coordinator_event_sequence_path: &str,
    coordinator_state_transition_path: &str,
    coordinator_terminal_action_path: &str,
    coordinator_tests_path: &str,
) {
    let coordinator = fs::read_to_string(coordinator_path).unwrap();
    let coordinator_approval = fs::read_to_string(coordinator_approval_path).unwrap();
    let coordinator_contracts = fs::read_to_string(coordinator_contracts_path).unwrap();
    let coordinator_event_sequence = fs::read_to_string(coordinator_event_sequence_path).unwrap();
    let coordinator_state_transition =
        fs::read_to_string(coordinator_state_transition_path).unwrap();
    let coordinator_terminal_action = fs::read_to_string(coordinator_terminal_action_path).unwrap();
    let coordinator_verification = fs::read_to_string(
        "src/runtime_core/workflow/application/transaction_coordinator/verification.rs",
    )
    .unwrap();
    let coordinator_tests = fs::read_to_string(coordinator_tests_path).unwrap();
    assert!(
        coordinator.contains("#[path = \"transaction_coordinator/tests.rs\"]"),
        "transaction coordinator does not register its regression-test owner"
    );
    for owner in [
        "approval",
        "contracts",
        "event_sequence",
        "state_transition",
        "terminal_action",
        "verification",
    ] {
        let declaration = format!("mod {owner};");
        assert!(
            coordinator.lines().any(|line| line == declaration),
            "transaction coordinator facade is missing child owner: {owner}"
        );
    }
    for (owner, rule) in [
        (&coordinator_approval, "fn execute_approval_transaction"),
        (
            &coordinator_verification,
            "fn execute_verification_transaction",
        ),
        (
            &coordinator_terminal_action,
            "fn execute_terminal_action_transaction",
        ),
        (&coordinator_state_transition, "fn execute_state_transition"),
        (
            &coordinator_state_transition,
            "fn execute_reconcile_transaction",
        ),
    ] {
        assert!(
            owner.contains(rule),
            "transaction coordinator owner is missing ordered use case: {rule}"
        );
        assert!(
            !coordinator.contains(rule),
            "transaction coordinator facade still owns ordered use case: {rule}"
        );
    }
    for definition in [
        "enum TransactionExecution",
        "enum ApprovalFault",
        "trait ApprovalTransactionPort",
        "enum VerificationFault",
        "trait VerificationTransactionPort",
        "enum TerminalActionFault",
        "trait TerminalActionTransactionPort",
        "enum StateTransitionFault",
        "trait StateTransitionTransactionPort",
        "trait ReconcileTransactionPort",
    ] {
        assert!(
            coordinator_contracts.contains(definition),
            "transaction contracts owner is missing: {definition}"
        );
        assert!(!coordinator.contains(definition));
    }
    for definition in ["struct PlannedEvent", "struct TransactionCoordinator"] {
        assert!(
            coordinator_event_sequence.contains(definition),
            "transaction event-sequence owner is missing: {definition}"
        );
        assert!(!coordinator.contains(definition));
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
    assert!(coordinator.lines().count() < 50);
    assert!(coordinator_approval.lines().count() < 100);
    assert!(coordinator_contracts.lines().count() < 225);
    assert!(coordinator_event_sequence.lines().count() < 100);
    assert!(coordinator_state_transition.lines().count() < 75);
    assert!(coordinator_terminal_action.lines().count() < 75);
    assert!(coordinator_verification.lines().count() < 75);
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
    for child in ["contracts", "projection", "transaction", "validation"] {
        let declaration = format!("mod {child};");
        assert!(
            recovery.lines().any(|line| line == declaration),
            "workflow recovery facade is missing child owner: {child}"
        );
    }
    assert!(
        recovery
            .split("#[cfg(test)]")
            .next()
            .unwrap()
            .lines()
            .count()
            < 50,
        "workflow recovery facade regrew beyond its ownership boundary"
    );
}
