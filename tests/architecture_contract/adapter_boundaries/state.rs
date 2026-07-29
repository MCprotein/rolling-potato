#[test]
fn v03713_state_adapter_separates_persistence_responsibilities() {
    let atomic_write_adapter = "src/adapters/filesystem/atomic_write.rs";
    let state_adapter = "src/app/workflow_adapter/state.rs";
    let current_snapshot_adapter = "src/app/workflow_adapter/state/current_snapshot.rs";
    let current_snapshot_codec = "src/app/workflow_adapter/state/current_snapshot/codec.rs";
    let current_snapshot_file_io = "src/app/workflow_adapter/state/current_snapshot/file_io.rs";
    let current_snapshot_lease_view =
        "src/app/workflow_adapter/state/current_snapshot/lease_view.rs";
    let current_snapshot_promotion = "src/app/workflow_adapter/state/current_snapshot/promotion.rs";
    let current_snapshot_status = "src/app/workflow_adapter/state/current_snapshot/status.rs";
    let current_transition_adapter = "src/app/workflow_adapter/state/current_transition.rs";
    let current_image_adapter =
        "src/app/workflow_adapter/state/current_transition/current_image.rs";
    let lifecycle_adapter = "src/app/workflow_adapter/state/lifecycle.rs";
    let lifecycle_events_adapter = "src/app/workflow_adapter/state/lifecycle/events.rs";
    let lifecycle_session_adapter = "src/app/workflow_adapter/state/lifecycle/session.rs";
    let lifecycle_state_commands_adapter =
        "src/app/workflow_adapter/state/lifecycle/state_commands.rs";
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
        "src/app/workflow_adapter/state/tests/current_snapshot/encoding_promotion.rs",
        "src/app/workflow_adapter/state/tests/current_snapshot/read_isolation.rs",
        "src/app/workflow_adapter/state/tests/current_snapshot/session_selection.rs",
        "src/app/workflow_adapter/state/tests/lifecycle.rs",
        "src/app/workflow_adapter/state/tests/lifecycle/bootstrap_session.rs",
        "src/app/workflow_adapter/state/tests/lifecycle/pointer_recovery.rs",
        "src/app/workflow_adapter/state/tests/lifecycle/workflow_recovery.rs",
        "src/app/workflow_adapter/state/tests/source_install.rs",
        "src/app/workflow_adapter/state/tests/workflow_store.rs",
    ];
    assert!(Path::new(atomic_write_adapter).is_file());
    assert!(Path::new(state_adapter).is_file());
    assert!(Path::new(current_snapshot_adapter).is_file());
    assert!(Path::new(current_snapshot_codec).is_file());
    assert!(Path::new(current_snapshot_file_io).is_file());
    assert!(Path::new(current_snapshot_lease_view).is_file());
    assert!(Path::new(current_snapshot_promotion).is_file());
    assert!(Path::new(current_snapshot_status).is_file());
    assert!(Path::new(current_transition_adapter).is_file());
    assert!(Path::new(current_image_adapter).is_file());
    assert!(Path::new(lifecycle_adapter).is_file());
    assert!(Path::new(lifecycle_events_adapter).is_file());
    assert!(Path::new(lifecycle_session_adapter).is_file());
    assert!(Path::new(lifecycle_state_commands_adapter).is_file());
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
    for module in [
        "mod codec;",
        "mod file_io;",
        "mod lease_view;",
        "mod promotion;",
        "mod status;",
    ] {
        assert!(
            current_snapshot.lines().any(|line| line == module),
            "current snapshot facade is missing responsibility owner: {module}"
        );
    }
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
    let current_snapshot_file_io = fs::read_to_string(current_snapshot_file_io).unwrap();
    for owned_responsibility in [
        "fn read_regular_file_bounded(",
        "fn read_open_file_bounded(",
        "fn validate_open_read_identity(",
    ] {
        assert!(
            current_snapshot_file_io.contains(owned_responsibility),
            "current snapshot file I/O owner is missing: {owned_responsibility}"
        );
    }
    let current_snapshot_lease_view = fs::read_to_string(current_snapshot_lease_view).unwrap();
    for owned_responsibility in [
        "fn current_state_lease_view(",
        "fn tui_state_snapshot_read_only(",
        "fn current_state_lease_view_under_transition(",
    ] {
        assert!(
            current_snapshot_lease_view.contains(owned_responsibility),
            "current snapshot lease-view owner is missing: {owned_responsibility}"
        );
    }
    let current_snapshot_promotion = fs::read_to_string(current_snapshot_promotion).unwrap();
    assert!(current_snapshot_promotion.contains("fn promote_current_state_v1("));
    let current_snapshot_status = fs::read_to_string(current_snapshot_status).unwrap();
    for owned_responsibility in [
        "fn read_current_state_summary(",
        "fn classify_current_state(",
        "enum CurrentStateStatus",
    ] {
        assert!(
            current_snapshot_status.contains(owned_responsibility),
            "current snapshot status owner is missing: {owned_responsibility}"
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
    for module in ["mod events;", "mod session;", "mod state_commands;"] {
        assert!(
            lifecycle.lines().any(|line| line == module),
            "state lifecycle facade is missing responsibility owner: {module}"
        );
    }
    let lifecycle_events = fs::read_to_string(lifecycle_events_adapter).unwrap();
    for owned_responsibility in [
        "pub fn record_event(",
        "pub(crate) fn current_compaction_boundary(",
        "pub(crate) fn record_compaction_boundary(",
    ] {
        assert!(
            lifecycle_events.contains(owned_responsibility),
            "state event lifecycle owner is missing responsibility: {owned_responsibility}"
        );
    }
    let lifecycle_session = fs::read_to_string(lifecycle_session_adapter).unwrap();
    for owned_responsibility in [
        "pub fn session_list_report(",
        "pub fn session_new_report(",
        "pub fn session_resume_report(",
    ] {
        assert!(
            lifecycle_session.contains(owned_responsibility),
            "session lifecycle owner is missing responsibility: {owned_responsibility}"
        );
    }
    let lifecycle_state_commands = fs::read_to_string(lifecycle_state_commands_adapter).unwrap();
    for owned_responsibility in [
        "pub fn initialize(",
        "pub fn reconcile_report(",
        "pub fn cancel_report(",
    ] {
        assert!(
            lifecycle_state_commands.contains(owned_responsibility),
            "state command lifecycle owner is missing responsibility: {owned_responsibility}"
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

    let current_snapshot_tests =
        fs::read_to_string("src/app/workflow_adapter/state/tests/current_snapshot.rs").unwrap();
    for owner in [
        "encoding_promotion",
        "read_isolation",
        "session_selection",
    ] {
        assert!(
            current_snapshot_tests.contains(&format!(
                "include!(\"current_snapshot/{owner}.rs\");"
            )),
            "current-snapshot test facade does not register {owner}"
        );
    }
    let lifecycle_tests =
        fs::read_to_string("src/app/workflow_adapter/state/tests/lifecycle.rs").unwrap();
    for owner in [
        "bootstrap_session",
        "pointer_recovery",
        "workflow_recovery",
    ] {
        assert!(
            lifecycle_tests.contains(&format!("include!(\"lifecycle/{owner}.rs\");")),
            "lifecycle test facade does not register {owner}"
        );
    }

    assert!(state.lines().count() < 450);
    assert!(current_snapshot.lines().count() < 50);
    assert!(current_snapshot_codec.lines().count() < 450);
    assert!(current_snapshot_file_io.lines().count() < 175);
    assert!(current_snapshot_lease_view.lines().count() < 325);
    assert!(current_snapshot_promotion.lines().count() < 250);
    assert!(current_snapshot_status.lines().count() < 100);
    assert!(current_transition.lines().count() < 400);
    assert!(current_image.lines().count() < 325);
    assert!(lifecycle.lines().count() < 30);
    assert!(lifecycle_events.lines().count() < 125);
    assert!(lifecycle_session.lines().count() < 375);
    assert!(lifecycle_state_commands.lines().count() < 275);
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
            tests.lines().count() < 500,
            "oversized state test module: {test_module}"
        );
    }
}
