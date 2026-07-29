use super::*;

#[test]
fn team_adapters_delegate_reports_execution_state_and_reconciliation() {
    assert_team_report_boundaries();
    assert_team_execution_boundaries();
    assert_team_state_boundaries();
    assert_team_facade_delegation();
    assert_collaboration_adapter_registration();
}

fn assert_team_report_boundaries() {
    let adapter_path = "src/app/collaboration_adapter/team.rs";
    let admission_path = "src/app/collaboration_adapter/team/admission.rs";
    let admission_report_path = "src/app/collaboration_adapter/team/admission_report.rs";
    let dispatch_path = "src/app/collaboration_adapter/team/dispatch.rs";
    let governor_path = "src/app/collaboration_adapter/team/governor.rs";
    let report_format_path = "src/app/collaboration_adapter/team/report_format.rs";
    let status_path = "src/app/collaboration_adapter/team/status.rs";
    let tests_path = "src/app/collaboration_adapter/team/tests.rs";
    for path in [
        adapter_path,
        admission_path,
        admission_report_path,
        dispatch_path,
        governor_path,
        report_format_path,
        status_path,
        tests_path,
    ] {
        assert_file(path);
    }

    let adapter = source(adapter_path);
    let admission = source(admission_path);
    let admission_report = source(admission_report_path);
    let dispatch = source(dispatch_path);
    let governor = source(governor_path);
    let report_format = source(report_format_path);
    let status = source(status_path);
    let tests = source(tests_path);

    assert_registered(&adapter, "mod admission;", "team adapter");
    for responsibility in [
        "pub(super) struct RecordedApprovalRequest",
        "pub(super) fn classify_policy_inputs(",
        "pub(super) fn normalize_ownership_claims(",
        "fn normalize_ownership_path(",
        "pub(super) fn record_approval_request(",
    ] {
        assert_moved(&admission, &adapter, responsibility);
    }
    for module in [
        "mod admission_report;",
        "mod dispatch;",
        "mod governor;",
        "mod report_format;",
        "mod status;",
    ] {
        assert_registered(&adapter, module, "team adapter");
    }
    for (owner, responsibility) in [
        (admission_report.as_str(), "pub fn admission_report("),
        (dispatch.as_str(), "pub fn dispatch_report("),
        (governor.as_str(), "pub fn governor_report("),
        (status.as_str(), "pub fn status_report("),
        (
            report_format.as_str(),
            "pub(super) fn latest_team_runtime_event(",
        ),
        (
            report_format.as_str(),
            "pub(super) fn format_policy_checks(",
        ),
    ] {
        assert_moved(owner, &adapter, responsibility);
    }
    assert!(
        adapter.contains("#[path = \"team/tests.rs\"]"),
        "team adapter does not register its regression-test owner"
    );
    for regression in [
        "fn admission_allows_parallel_and_records_ledger_event(",
        "fn admission_blocks_cross_lane_file_ownership_conflict(",
        "fn dispatch_enforces_file_ownership_at_dispatch_time(",
        "fn governor_blocks_critical_pressure_and_records_ledger_event(",
    ] {
        assert_moved(&tests, &adapter, regression);
    }

    assert!(adapter.lines().count() <= 50);
    for (owner, owner_source, limit) in [
        (admission_path, admission.as_str(), 250),
        (admission_report_path, admission_report.as_str(), 175),
        (dispatch_path, dispatch.as_str(), 175),
        (governor_path, governor.as_str(), 150),
        (report_format_path, report_format.as_str(), 150),
        (status_path, status.as_str(), 125),
        (tests_path, tests.as_str(), 525),
    ] {
        assert_line_bound(owner_source, limit, owner);
    }
}

fn assert_team_execution_boundaries() {
    let adapter_path = "src/app/collaboration_adapter/team_execution.rs";
    let admission_path = "src/app/collaboration_adapter/team_execution/admission.rs";
    let events_path = "src/app/collaboration_adapter/team_execution/events.rs";
    let tests_path = "src/app/collaboration_adapter/team_execution/tests.rs";
    for path in [adapter_path, admission_path, events_path, tests_path] {
        assert_file(path);
    }
    let adapter = source(adapter_path);
    let admission = source(admission_path);
    let events = source(events_path);
    let tests = source(tests_path);

    assert_registered(&adapter, "mod admission;", "team execution adapter");
    assert_registered(&adapter, "mod events;", "team execution adapter");
    for responsibility in [
        "pub(super) fn recover_or_admit_execution(",
        "pub(super) fn team_launches(",
        "fn admitted_worker_bindings(",
        "fn fail_interrupted_execution(",
        "pub(super) fn enforce_action_ownership(",
    ] {
        assert_moved(&admission, &adapter, responsibility);
    }
    for responsibility in [
        "pub(super) fn append_execution_blocked(",
        "pub(super) fn append_action_event(",
        "pub(super) fn append_worker_event(",
        "fn has_exact_event(",
    ] {
        assert_moved(&events, &adapter, responsibility);
    }
    assert!(
        adapter.contains("#[path = \"team_execution/tests.rs\"]"),
        "team execution adapter does not register its regression-test owner"
    );
    for regression in [
        "fn dispatch_retry_resumes_fully_admitted_workers_without_duplicate_admission(",
        "fn cancel_cannot_cross_the_admission_operation_barrier(",
        "fn worker_failure_collects_remaining_results_and_terminalizes_team(",
        "fn source_change_after_worker_completion_blocks_before_parent_evidence_merge(",
    ] {
        assert_moved(&tests, &adapter, regression);
    }

    assert!(adapter.lines().count() <= 325);
    assert_line_bound(&admission, 300, admission_path);
    assert_line_bound(&events, 125, events_path);
    assert_line_bound(&tests, 650, tests_path);
}

fn assert_team_state_boundaries() {
    let adapter_path = "src/app/collaboration_adapter/team_state.rs";
    let events_path = "src/app/collaboration_adapter/team_state/events.rs";
    let persistence_path = "src/app/collaboration_adapter/team_state/persistence.rs";
    let tests_path = "src/app/collaboration_adapter/team_state/tests.rs";
    for path in [adapter_path, events_path, persistence_path, tests_path] {
        assert_file(path);
    }
    let adapter = source(adapter_path);
    let events = source(events_path);
    let persistence = source(persistence_path);
    let tests = source(tests_path);

    assert_registered(&adapter, "mod events;", "team state adapter");
    assert_registered(&adapter, "mod persistence;", "team state adapter");
    for responsibility in [
        "pub(super) fn append_planned_event_if_missing(",
        "pub(super) fn append_stage_event_if_missing(",
    ] {
        assert_moved(&events, &adapter, responsibility);
    }
    for responsibility in [
        "pub(super) fn install_cancel_marker(",
        "pub(super) fn parse_cancel_marker(",
        "pub(super) fn install_manifest(",
        "pub(super) fn load_state_unlocked(",
        "pub(super) fn install_snapshot(",
        "pub(super) fn verify_snapshot_chain(",
    ] {
        assert_moved(&persistence, &adapter, responsibility);
    }
    assert!(
        adapter.contains("#[path = \"team_state/tests.rs\"]"),
        "team state adapter does not register its regression-test owner"
    );
    for regression in [
        "fn plan_persists_canonical_manifest_and_hash_chained_state(",
        "fn stage_machine_allows_only_ordered_runtime_transitions(",
        "fn cancellation_marker_is_durable_idempotent_and_hash_bound(",
        "fn tampered_current_state_is_rejected_against_artifact_hash(",
    ] {
        assert_moved(&tests, &adapter, regression);
    }

    assert!(adapter.lines().count() <= 400);
    assert_line_bound(&events, 100, events_path);
    assert_line_bound(&persistence, 250, persistence_path);
    assert_line_bound(&tests, 225, tests_path);
}

fn assert_team_facade_delegation() {
    let reconciliation_path = "src/app/collaboration_adapter/team_reconciliation.rs";
    for (facade_path, moved_definition, delegation) in [
        (
            "src/app/collaboration_adapter/team.rs",
            "struct ContinuationDecision",
            "collaboration::team",
        ),
        (
            "src/app/collaboration_adapter/team.rs",
            "struct PolicyGate",
            "collaboration::team",
        ),
        (
            "src/app/collaboration_adapter/team.rs",
            "fn policy_preflight",
            "collaboration::team",
        ),
        (
            "src/app/collaboration_adapter/team.rs",
            "fn ownership_preflight",
            "collaboration::team",
        ),
        (
            "src/app/collaboration_adapter/team.rs",
            "fn admission_summary",
            "collaboration::team",
        ),
        (
            "src/app/collaboration_adapter/team_execution.rs",
            "fn pressure_from_status",
            "validate_execution_binding",
        ),
        (
            "src/app/collaboration_adapter/team_execution.rs",
            "fn record_matches_team",
            "validate_execution_binding",
        ),
        (
            "src/app/collaboration_adapter/team_reconciliation.rs",
            "fn validate_team_binding",
            "validate_reconciliation_binding",
        ),
        (
            "src/app/collaboration_adapter/team_reconciliation.rs",
            "fn validate_member_record",
            "validate_reconciliation_binding",
        ),
        (
            "src/app/collaboration_adapter/team_state.rs",
            "pub enum TeamStage",
            "collaboration::team_state",
        ),
        (
            "src/app/collaboration_adapter/team_state.rs",
            "fn parse_members",
            "collaboration::team_state",
        ),
        (
            "src/app/collaboration_adapter/team_state.rs",
            "fn render_state",
            "collaboration::team_state",
        ),
    ] {
        let facade = source(facade_path);
        let production = facade.split("#[cfg(test)]").next().unwrap_or(&facade);
        assert!(
            !production.contains(moved_definition),
            "team facade retains moved rule: {facade_path} -> {moved_definition}"
        );
        assert!(
            facade.contains(delegation),
            "team facade is missing owner delegation: {facade_path} -> {delegation}"
        );
    }
    assert_line_bound(&source(reconciliation_path), 551, reconciliation_path);
}

fn assert_collaboration_adapter_registration() {
    let adapter_mod = source("src/app/collaboration_adapter.rs");
    for child in [
        "subagent",
        "team",
        "team_execution",
        "team_reconciliation",
        "team_state",
    ] {
        assert_registered(
            &adapter_mod,
            &format!("pub(crate) mod {child};"),
            "collaboration adapter",
        );
    }

    assert_eq!(
        source("tests/subagent_lifecycle.rs").trim(),
        "include!(\"collaboration/subagent_lifecycle.rs\");"
    );
    assert_eq!(
        source("tests/team_runtime.rs").trim(),
        "include!(\"collaboration/team_runtime.rs\");"
    );
    assert_file("src/app/collaboration_adapter.rs");
    assert_file("src/app/collaboration_adapter/subagent_result.rs");
}
