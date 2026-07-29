use super::*;

#[test]
fn v03712_collaboration_owners_hold_lifecycle_execution_and_reconciliation_policy() {
    let subagent_adapter = "src/app/collaboration_adapter/subagent.rs";
    let subagent_execution = "src/app/collaboration_adapter/subagent/execution.rs";
    let subagent_persistence = "src/app/collaboration_adapter/subagent/persistence.rs";
    let subagent_tests = "src/app/collaboration_adapter/subagent/tests.rs";
    let subagent_launch = "src/runtime_core/collaboration/subagent/launch.rs";
    let subagent_record_codec = "src/runtime_core/collaboration/subagent/record_codec.rs";
    let subagent_result_evidence = "src/runtime_core/collaboration/subagent_result/evidence.rs";
    let team_adapter = "src/app/collaboration_adapter/team.rs";
    let team_admission = "src/app/collaboration_adapter/team/admission.rs";
    let team_admission_report = "src/app/collaboration_adapter/team/admission_report.rs";
    let team_dispatch = "src/app/collaboration_adapter/team/dispatch.rs";
    let team_governor = "src/app/collaboration_adapter/team/governor.rs";
    let team_report_format = "src/app/collaboration_adapter/team/report_format.rs";
    let team_status = "src/app/collaboration_adapter/team/status.rs";
    let team_tests = "src/app/collaboration_adapter/team/tests.rs";
    let team_execution_adapter = "src/app/collaboration_adapter/team_execution.rs";
    let team_execution_admission = "src/app/collaboration_adapter/team_execution/admission.rs";
    let team_execution_events = "src/app/collaboration_adapter/team_execution/events.rs";
    let team_execution_tests = "src/app/collaboration_adapter/team_execution/tests.rs";
    let team_reconciliation_adapter = "src/app/collaboration_adapter/team_reconciliation.rs";
    let team_state_adapter = "src/app/collaboration_adapter/team_state.rs";
    let team_state_events = "src/app/collaboration_adapter/team_state/events.rs";
    let team_state_persistence = "src/app/collaboration_adapter/team_state/persistence.rs";
    let team_state_tests = "src/app/collaboration_adapter/team_state/tests.rs";
    let owners: &[(&str, &[&str])] = &[
        (
            "src/runtime_core/collaboration/subagent.rs",
            &[
                "enum SubagentRole",
                "enum SubagentStatus",
                "struct SubagentRecordV1",
                "fn validate_record",
            ],
        ),
        (
            "src/runtime_core/collaboration/subagent_result.rs",
            &[
                "struct SubagentResultV1",
                "fn parse_result_shape",
                "fn validate_patch_policy",
                "fn validate_context_binding",
                "fn validate_bounded_text",
            ],
        ),
        (
            "src/runtime_core/collaboration/team.rs",
            &[
                "struct ContinuationDecision",
                "struct PolicyGate",
                "fn continuation_decision",
                "fn evaluate_policy_gate",
                "fn evaluate_ownership_gate",
                "fn dispatch_event_type",
                "fn admission_summary",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_execution.rs",
            &[
                "fn validate_execution_binding",
                "fn validate_execution_stage",
                "fn execution_mode",
                "fn validate_action_owner",
                "fn record_matches_team",
                "fn validate_completed_member_binding",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_reconciliation.rs",
            &[
                "fn validate_reconciliation_binding",
                "fn validate_reconciliation_stage",
                "fn validate_action_ownership",
                "fn validate_member_record",
                "fn render_reconciliation",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_state.rs",
            &[
                "enum TeamStage",
                "fn transition_to_at",
                "fn parse_manifest",
                "fn parse_state",
                "fn render_state",
            ],
        ),
    ];
    let collaboration_mod = fs::read_to_string("src/runtime_core/collaboration/mod.rs").unwrap();
    for (owner, rules) in owners {
        assert!(
            Path::new(owner).is_file(),
            "missing v0.37.12 collaboration owner: {owner}"
        );
        let child = Path::new(owner).file_stem().unwrap().to_str().unwrap();
        let expected = format!("pub(crate) mod {child};");
        assert!(
            collaboration_mod.lines().any(|line| line == expected),
            "collaboration child is not crate-private: {child}"
        );
        let source = fs::read_to_string(owner).unwrap();
        for rule in *rules {
            assert!(
                source.contains(rule),
                "v0.37.12 owner is missing collaboration rule: {owner} -> {rule}"
            );
        }
        for dependency in [
            "crate::adapters",
            "crate::backend",
            "crate::ledger",
            "crate::observability",
            "crate::state",
            "std::fs",
            "std::process",
            "std::thread",
        ] {
            assert!(
                !source.contains(dependency),
                "collaboration owner has concrete reverse dependency: {owner} -> {dependency}"
            );
        }
    }

    assert!(Path::new(subagent_execution).is_file());
    assert!(Path::new(subagent_persistence).is_file());
    assert!(Path::new(subagent_tests).is_file());
    assert!(Path::new(subagent_launch).is_file());
    assert!(Path::new(subagent_record_codec).is_file());
    assert!(Path::new(subagent_result_evidence).is_file());
    assert!(Path::new(team_admission).is_file());
    assert!(Path::new(team_admission_report).is_file());
    assert!(Path::new(team_dispatch).is_file());
    assert!(Path::new(team_governor).is_file());
    assert!(Path::new(team_report_format).is_file());
    assert!(Path::new(team_status).is_file());
    assert!(Path::new(team_tests).is_file());
    assert!(Path::new(team_execution_admission).is_file());
    assert!(Path::new(team_execution_events).is_file());
    assert!(Path::new(team_execution_tests).is_file());
    assert!(Path::new(team_state_events).is_file());
    assert!(Path::new(team_state_persistence).is_file());
    assert!(Path::new(team_state_tests).is_file());
    let subagent_source = fs::read_to_string(subagent_adapter).unwrap();
    let subagent_domain = fs::read_to_string("src/runtime_core/collaboration/subagent.rs").unwrap();
    let subagent_launch_source = fs::read_to_string(subagent_launch).unwrap();
    let subagent_record_codec_source = fs::read_to_string(subagent_record_codec).unwrap();
    let subagent_result_source =
        fs::read_to_string("src/runtime_core/collaboration/subagent_result.rs").unwrap();
    let subagent_result_evidence_source = fs::read_to_string(subagent_result_evidence).unwrap();
    let subagent_execution_source = fs::read_to_string(subagent_execution).unwrap();
    let subagent_persistence_source = fs::read_to_string(subagent_persistence).unwrap();
    let subagent_test_source = fs::read_to_string(subagent_tests).unwrap();
    let team_source = fs::read_to_string(team_adapter).unwrap();
    let team_admission_source = fs::read_to_string(team_admission).unwrap();
    let team_admission_report_source = fs::read_to_string(team_admission_report).unwrap();
    let team_dispatch_source = fs::read_to_string(team_dispatch).unwrap();
    let team_governor_source = fs::read_to_string(team_governor).unwrap();
    let team_report_format_source = fs::read_to_string(team_report_format).unwrap();
    let team_status_source = fs::read_to_string(team_status).unwrap();
    let team_test_source = fs::read_to_string(team_tests).unwrap();
    let team_execution_source = fs::read_to_string(team_execution_adapter).unwrap();
    let team_execution_admission_source = fs::read_to_string(team_execution_admission).unwrap();
    let team_execution_events_source = fs::read_to_string(team_execution_events).unwrap();
    let team_execution_test_source = fs::read_to_string(team_execution_tests).unwrap();
    let team_state_source = fs::read_to_string(team_state_adapter).unwrap();
    let team_state_event_source = fs::read_to_string(team_state_events).unwrap();
    let team_state_persistence_source = fs::read_to_string(team_state_persistence).unwrap();
    let team_state_test_source = fs::read_to_string(team_state_tests).unwrap();
    assert!(
        subagent_source.lines().any(|line| line == "mod execution;"),
        "subagent adapter does not register its execution owner"
    );
    assert!(
        subagent_domain.lines().any(|line| line == "mod launch;"),
        "subagent domain does not register its launch policy owner"
    );
    for responsibility in [
        "pub fn validate_launch(",
        "pub(crate) fn normalize_tools(",
        "pub(crate) fn normalize_paths(",
        "pub(crate) fn normalize_relative_path(",
    ] {
        assert!(
            subagent_launch_source.contains(responsibility),
            "subagent launch policy owner is missing: {responsibility}"
        );
        assert!(
            !subagent_domain.contains(responsibility),
            "subagent record domain still owns launch policy: {responsibility}"
        );
    }
    assert!(
        subagent_domain
            .lines()
            .any(|line| line == "mod record_codec;"),
        "subagent domain does not register its record codec owner"
    );
    for responsibility in [
        "pub(crate) fn render_payload(",
        "pub(crate) fn render_record(",
        "pub(crate) fn parse_record(",
        "fn canonical_string(",
        "fn canonical_string_array(",
    ] {
        assert!(
            subagent_record_codec_source.contains(responsibility),
            "subagent record codec owner is missing: {responsibility}"
        );
        assert!(
            !subagent_domain.contains(responsibility),
            "subagent domain still owns record codec behavior: {responsibility}"
        );
    }
    assert!(
        subagent_result_source
            .lines()
            .any(|line| line == "mod evidence;"),
        "subagent result domain does not register its evidence policy owner"
    );
    for responsibility in [
        "const EVIDENCE_V2_KEYS",
        "pub(crate) fn evidence_source_bindings(",
        "pub(crate) fn verify_evidence_artifact(",
        "pub(crate) fn render_evidence_payload_v2(",
        "pub(crate) fn evidence_id(",
        "pub(crate) fn installable_evidence_body(",
    ] {
        assert!(
            subagent_result_evidence_source.contains(responsibility),
            "subagent evidence policy owner is missing: {responsibility}"
        );
        assert!(
            !subagent_result_source.contains(responsibility),
            "subagent result domain still owns evidence policy: {responsibility}"
        );
    }
    assert!(
        subagent_source
            .lines()
            .any(|line| line == "mod persistence;"),
        "subagent adapter does not register its persistence owner"
    );
    assert!(
        subagent_source.contains("#[path = \"subagent/tests.rs\"]"),
        "subagent adapter does not register its regression-test owner"
    );
    for regression in [
        "fn launch_contract_enforces_role_tool_and_write_boundaries(",
        "fn canonical_state_round_trips_and_preserves_hash_chain(",
        "fn dispatch_completes_and_merges_evidence_once(",
        "fn stale_running_child_recovers_as_failed_without_backend_replay(",
    ] {
        assert!(
            subagent_test_source.contains(regression),
            "subagent regression owner is missing: {regression}"
        );
    }
    assert!(
        team_state_source.lines().any(|line| line == "mod events;"),
        "team state adapter does not register its event persistence owner"
    );
    for responsibility in [
        "pub(super) fn append_planned_event_if_missing(",
        "pub(super) fn append_stage_event_if_missing(",
    ] {
        assert!(
            team_state_event_source.contains(responsibility),
            "team state event owner is missing: {responsibility}"
        );
        assert!(
            !team_state_source.contains(responsibility),
            "team state adapter still owns event persistence: {responsibility}"
        );
    }
    assert!(
        team_state_source
            .lines()
            .any(|line| line == "mod persistence;"),
        "team state adapter does not register its persistence owner"
    );
    for responsibility in [
        "pub(super) fn install_cancel_marker(",
        "pub(super) fn parse_cancel_marker(",
        "pub(super) fn install_manifest(",
        "pub(super) fn load_state_unlocked(",
        "pub(super) fn install_snapshot(",
        "pub(super) fn verify_snapshot_chain(",
    ] {
        assert!(
            team_state_persistence_source.contains(responsibility),
            "team state persistence owner is missing: {responsibility}"
        );
        assert!(
            !team_state_source.contains(responsibility),
            "team state adapter still owns persistence: {responsibility}"
        );
    }
    assert!(
        team_state_source.contains("#[path = \"team_state/tests.rs\"]"),
        "team state adapter does not register its regression-test owner"
    );
    for regression in [
        "fn plan_persists_canonical_manifest_and_hash_chained_state(",
        "fn stage_machine_allows_only_ordered_runtime_transitions(",
        "fn cancellation_marker_is_durable_idempotent_and_hash_bound(",
        "fn tampered_current_state_is_rejected_against_artifact_hash(",
    ] {
        assert!(
            team_state_test_source.contains(regression),
            "team state regression owner is missing: {regression}"
        );
        assert!(
            !team_state_source.contains(regression),
            "team state adapter still owns regression test: {regression}"
        );
    }
    assert!(
        team_source.lines().any(|line| line == "mod admission;"),
        "team adapter does not register its admission preparation owner"
    );
    for responsibility in [
        "pub(super) struct RecordedApprovalRequest",
        "pub(super) fn classify_policy_inputs(",
        "pub(super) fn normalize_ownership_claims(",
        "fn normalize_ownership_path(",
        "pub(super) fn record_approval_request(",
    ] {
        assert!(
            team_admission_source.contains(responsibility),
            "team admission owner is missing: {responsibility}"
        );
        assert!(
            !team_source.contains(responsibility),
            "team adapter still owns admission preparation: {responsibility}"
        );
    }
    for module in [
        "mod admission_report;",
        "mod dispatch;",
        "mod governor;",
        "mod report_format;",
        "mod status;",
    ] {
        assert!(
            team_source.lines().any(|line| line == module),
            "team adapter does not register report owner: {module}"
        );
    }
    for (source, responsibility) in [
        (
            team_admission_report_source.as_str(),
            "pub fn admission_report(",
        ),
        (team_dispatch_source.as_str(), "pub fn dispatch_report("),
        (team_governor_source.as_str(), "pub fn governor_report("),
        (team_status_source.as_str(), "pub fn status_report("),
        (
            team_report_format_source.as_str(),
            "pub(super) fn latest_team_runtime_event(",
        ),
        (
            team_report_format_source.as_str(),
            "pub(super) fn format_policy_checks(",
        ),
    ] {
        assert!(
            source.contains(responsibility),
            "team report owner is missing: {responsibility}"
        );
        assert!(
            !team_source.contains(responsibility),
            "team facade still owns report behavior: {responsibility}"
        );
    }
    for responsibility in [
        "pub(crate) struct WorkerGeneration",
        "pub(crate) struct PreparedTeamMember",
        "pub(crate) struct CompletedTeamMember",
        "pub(crate) fn terminalize_interrupted_team_members(",
        "pub(crate) fn execute_admitted_team_member_with(",
        "pub(crate) fn prepare_team_members(",
        "pub(crate) fn execute_prepared_team_member_with(",
        "fn execute_prepared_launch(",
        "fn complete_generation(",
        "fn merge_completed_result(",
        "fn recover_completed_parent_merges(",
    ] {
        assert!(
            subagent_execution_source.contains(responsibility),
            "subagent execution owner is missing: {responsibility}"
        );
        assert!(
            !subagent_source.contains(responsibility),
            "subagent adapter still owns execution: {responsibility}"
        );
    }
    for responsibility in [
        "impl SubagentRecordV1",
        "pub fn create_record(",
        "pub fn checkpoint_record(",
        "pub fn load_record(",
        "pub(crate) fn records_for_parent(",
        "fn load_record_unlocked(",
        "fn install_snapshot(",
        "fn verify_snapshot_chain(",
    ] {
        assert!(
            subagent_persistence_source.contains(responsibility),
            "subagent persistence owner is missing: {responsibility}"
        );
        assert!(
            !subagent_source.contains(responsibility),
            "subagent adapter still owns persistence: {responsibility}"
        );
    }
    assert!(
        team_source.contains("#[path = \"team/tests.rs\"]"),
        "team adapter does not register its regression-test owner"
    );
    for regression in [
        "fn admission_allows_parallel_and_records_ledger_event(",
        "fn admission_blocks_cross_lane_file_ownership_conflict(",
        "fn dispatch_enforces_file_ownership_at_dispatch_time(",
        "fn governor_blocks_critical_pressure_and_records_ledger_event(",
    ] {
        assert!(
            team_test_source.contains(regression),
            "team regression owner is missing: {regression}"
        );
        assert!(
            !team_source.contains(regression),
            "team adapter still owns regression test: {regression}"
        );
    }
    assert!(
        team_execution_source
            .lines()
            .any(|line| line == "mod admission;"),
        "team execution adapter does not register its admission recovery owner"
    );
    for responsibility in [
        "pub(super) fn recover_or_admit_execution(",
        "pub(super) fn team_launches(",
        "fn admitted_worker_bindings(",
        "fn fail_interrupted_execution(",
        "pub(super) fn enforce_action_ownership(",
    ] {
        assert!(
            team_execution_admission_source.contains(responsibility),
            "team execution admission owner is missing: {responsibility}"
        );
        assert!(
            !team_execution_source.contains(responsibility),
            "team execution adapter still owns admission recovery: {responsibility}"
        );
    }
    assert!(
        team_execution_source
            .lines()
            .any(|line| line == "mod events;"),
        "team execution adapter does not register its event persistence owner"
    );
    for responsibility in [
        "pub(super) fn append_execution_blocked(",
        "pub(super) fn append_action_event(",
        "pub(super) fn append_worker_event(",
        "fn has_exact_event(",
    ] {
        assert!(
            team_execution_events_source.contains(responsibility),
            "team execution event owner is missing: {responsibility}"
        );
        assert!(
            !team_execution_source.contains(responsibility),
            "team execution adapter still owns event persistence: {responsibility}"
        );
    }
    assert!(
        team_execution_source.contains("#[path = \"team_execution/tests.rs\"]"),
        "team execution adapter does not register its regression-test owner"
    );
    for regression in [
        "fn dispatch_retry_resumes_fully_admitted_workers_without_duplicate_admission(",
        "fn cancel_cannot_cross_the_admission_operation_barrier(",
        "fn worker_failure_collects_remaining_results_and_terminalizes_team(",
        "fn source_change_after_worker_completion_blocks_before_parent_evidence_merge(",
    ] {
        assert!(
            team_execution_test_source.contains(regression),
            "team execution regression owner is missing: {regression}"
        );
        assert!(
            !team_execution_source.contains(regression),
            "team execution adapter still owns regression test: {regression}"
        );
    }

    for (facade, moved_definition) in [
        (subagent_adapter, "pub enum SubagentRole"),
        (subagent_adapter, "pub struct SubagentRecordV1"),
        (subagent_adapter, "fn validate_record"),
        (subagent_adapter, "fn render_record"),
        (subagent_adapter, "fn normalize_paths"),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "const RESULT_KEYS",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "const EVIDENCE_V2_KEYS",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "fn validate_patch",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "fn verify_evidence_artifact",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "fn render_evidence_payload_v2",
        ),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "fn validate_bounded_text",
        ),
        (team_adapter, "struct ContinuationDecision"),
        (team_adapter, "struct PolicyGate"),
        (team_adapter, "fn policy_preflight"),
        (team_adapter, "fn ownership_preflight"),
        (team_adapter, "fn admission_summary"),
        (team_execution_adapter, "fn pressure_from_status"),
        (team_execution_adapter, "fn record_matches_team"),
        (team_reconciliation_adapter, "fn validate_team_binding"),
        (team_reconciliation_adapter, "fn validate_member_record"),
        (team_state_adapter, "pub enum TeamStage"),
        (team_state_adapter, "fn parse_members"),
        (team_state_adapter, "fn render_state"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(moved_definition),
            "legacy collaboration facade retains moved rule: {facade} -> {moved_definition}"
        );
    }

    for (facade, delegation) in [
        (subagent_adapter, "collaboration::subagent::*"),
        (
            "src/app/collaboration_adapter/subagent_result.rs",
            "result_policy::parse_result_shape",
        ),
        (team_adapter, "collaboration::team"),
        (team_execution_adapter, "validate_execution_binding"),
        (
            team_reconciliation_adapter,
            "validate_reconciliation_binding",
        ),
        (team_state_adapter, "collaboration::team_state"),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            source.contains(delegation),
            "legacy collaboration facade is missing owner delegation: {facade} -> {delegation}"
        );
    }

    for (facade, maximum_lines) in [
        (subagent_adapter, 500),
        ("src/app/collaboration_adapter/subagent_result.rs", 800),
        (team_adapter, 50),
        (team_execution_adapter, 325),
        (team_reconciliation_adapter, 550),
        (team_state_adapter, 400),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        assert!(
            source.lines().count() <= maximum_lines,
            "collaboration facade regrew beyond the v0.37.12 boundary: {facade}"
        );
    }
    assert!(
        subagent_execution_source.lines().count() < 600,
        "subagent execution module regrew beyond its ownership boundary"
    );
    assert!(
        subagent_domain.lines().count() < 450,
        "subagent domain regrew beyond its ownership boundary"
    );
    assert!(subagent_launch_source.lines().count() < 225);
    assert!(
        subagent_record_codec_source.lines().count() < 250,
        "subagent record codec regrew beyond its ownership boundary"
    );
    assert!(
        subagent_result_source.lines().count() < 350,
        "subagent result policy regrew beyond its ownership boundary"
    );
    assert!(
        subagent_result_evidence_source.lines().count() < 300,
        "subagent evidence policy regrew beyond its ownership boundary"
    );
    assert!(
        subagent_persistence_source.lines().count() < 325,
        "subagent persistence module regrew beyond its ownership boundary"
    );
    assert!(
        subagent_test_source.lines().count() < 675,
        "subagent regression module regrew beyond its ownership boundary"
    );
    assert!(
        team_test_source.lines().count() < 525,
        "team regression module regrew beyond its ownership boundary"
    );
    assert!(
        team_admission_source.lines().count() < 250,
        "team admission module regrew beyond its ownership boundary"
    );
    for (source, maximum_lines, owner) in [
        (
            team_admission_report_source.as_str(),
            175,
            team_admission_report,
        ),
        (team_dispatch_source.as_str(), 175, team_dispatch),
        (team_governor_source.as_str(), 150, team_governor),
        (team_report_format_source.as_str(), 150, team_report_format),
        (team_status_source.as_str(), 125, team_status),
    ] {
        assert!(
            source.lines().count() < maximum_lines,
            "team report module regrew beyond its ownership boundary: {owner}"
        );
    }
    assert!(
        team_execution_admission_source.lines().count() < 300,
        "team execution admission module regrew beyond its ownership boundary"
    );
    assert!(
        team_execution_events_source.lines().count() < 125,
        "team execution event module regrew beyond its ownership boundary"
    );
    assert!(
        team_execution_test_source.lines().count() < 650,
        "team execution regression module regrew beyond its ownership boundary"
    );
    assert!(
        team_state_event_source.lines().count() < 100,
        "team state event module regrew beyond its ownership boundary"
    );
    assert!(
        team_state_persistence_source.lines().count() < 250,
        "team state persistence module regrew beyond its ownership boundary"
    );
    assert!(
        team_state_test_source.lines().count() < 225,
        "team state regression module regrew beyond its ownership boundary"
    );

    for legacy in [
        "src/subagent.rs",
        "src/team.rs",
        "src/team_execution.rs",
        "src/team_reconciliation.rs",
        "src/team_state.rs",
    ] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy collaboration root was restored: {legacy}"
        );
    }
    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_mod in [
        "mod subagent;",
        "mod team;",
        "mod team_execution;",
        "mod team_reconciliation;",
        "mod team_state;",
        "pub mod team_state;",
    ] {
        assert!(
            !main.lines().any(|line| line == legacy_mod),
            "legacy collaboration root remains registered: {legacy_mod}"
        );
    }
    let adapter_mod = fs::read_to_string("src/app/collaboration_adapter.rs").unwrap();
    for child in [
        "subagent",
        "team",
        "team_execution",
        "team_reconciliation",
        "team_state",
    ] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            adapter_mod.lines().any(|line| line == expected),
            "collaboration adapter is not registered: {child}"
        );
    }

    assert_eq!(
        fs::read_to_string("tests/subagent_lifecycle.rs")
            .unwrap()
            .trim(),
        "include!(\"collaboration/subagent_lifecycle.rs\");"
    );
    assert_eq!(
        fs::read_to_string("tests/team_runtime.rs").unwrap().trim(),
        "include!(\"collaboration/team_runtime.rs\");"
    );
    assert!(!Path::new("src/subagent_result.rs").exists());
    assert!(Path::new("src/app/collaboration_adapter.rs").is_file());
    assert!(Path::new("src/app/collaboration_adapter/subagent_result.rs").is_file());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod subagent_result;"));
}
