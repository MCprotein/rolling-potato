use super::*;

#[test]
fn subagent_adapter_delegates_execution_persistence_and_regressions() {
    let adapter_path = "src/app/collaboration_adapter/subagent.rs";
    let admission_path = "src/app/collaboration_adapter/subagent/admission.rs";
    let execution_path = "src/app/collaboration_adapter/subagent/execution.rs";
    let completion_path = "src/app/collaboration_adapter/subagent/execution/completion.rs";
    let dispatch_path = "src/app/collaboration_adapter/subagent/execution/dispatch.rs";
    let member_path = "src/app/collaboration_adapter/subagent/execution/member.rs";
    let parent_merge_path = "src/app/collaboration_adapter/subagent/execution/parent_merge.rs";
    let lifecycle_path = "src/app/collaboration_adapter/subagent/lifecycle.rs";
    let persistence_path = "src/app/collaboration_adapter/subagent/persistence.rs";
    let reporting_path = "src/app/collaboration_adapter/subagent/reporting.rs";
    let tests_path = "src/app/collaboration_adapter/subagent/tests.rs";
    let test_admission_path = "src/app/collaboration_adapter/subagent/tests/admission.rs";
    let test_contract_path = "src/app/collaboration_adapter/subagent/tests/contract.rs";
    let test_execution_path = "src/app/collaboration_adapter/subagent/tests/execution.rs";
    let test_persistence_path = "src/app/collaboration_adapter/subagent/tests/persistence.rs";

    for path in [
        adapter_path,
        admission_path,
        execution_path,
        completion_path,
        dispatch_path,
        member_path,
        parent_merge_path,
        lifecycle_path,
        persistence_path,
        reporting_path,
        tests_path,
        test_admission_path,
        test_contract_path,
        test_execution_path,
        test_persistence_path,
    ] {
        assert_file(path);
    }

    let adapter = source(adapter_path);
    let admission = source(admission_path);
    let execution = source(execution_path);
    let completion = source(completion_path);
    let dispatch = source(dispatch_path);
    let member = source(member_path);
    let parent_merge = source(parent_merge_path);
    let lifecycle = source(lifecycle_path);
    let persistence = source(persistence_path);
    let reporting = source(reporting_path);
    let tests = source(tests_path);
    let test_admission = source(test_admission_path);
    let test_contract = source(test_contract_path);
    let test_execution = source(test_execution_path);
    let test_persistence = source(test_persistence_path);

    assert_registered(&adapter, "mod admission;", "subagent adapter");
    assert_registered(&adapter, "mod execution;", "subagent adapter");
    assert_registered(&adapter, "mod lifecycle;", "subagent adapter");
    assert_registered(&adapter, "mod persistence;", "subagent adapter");
    assert_registered(&adapter, "mod reporting;", "subagent adapter");
    assert!(
        adapter.contains("#[path = \"subagent/tests.rs\"]"),
        "subagent adapter does not register its regression-test owner"
    );
    assert_execution_boundaries(
        &adapter,
        &execution,
        &completion,
        &dispatch,
        &member,
        &parent_merge,
    );
    assert_admission_boundaries(&adapter, &admission);
    assert_lifecycle_and_reporting_boundaries(&adapter, &lifecycle, &reporting);
    assert_persistence_boundaries(&adapter, &persistence);
    assert_regression_boundaries(
        &tests,
        &test_admission,
        &test_contract,
        &test_execution,
        &test_persistence,
    );
    assert_facade_delegation(&adapter);

    assert_line_bound(&adapter, 75, adapter_path);
    for (owner, owner_source, limit) in [
        (admission_path, admission.as_str(), 350),
        (execution_path, execution.as_str(), 100),
        (completion_path, completion.as_str(), 225),
        (dispatch_path, dispatch.as_str(), 175),
        (member_path, member.as_str(), 175),
        (parent_merge_path, parent_merge.as_str(), 125),
        (lifecycle_path, lifecycle.as_str(), 100),
        (persistence_path, persistence.as_str(), 325),
        (reporting_path, reporting.as_str(), 125),
        (tests_path, tests.as_str(), 100),
        (test_admission_path, test_admission.as_str(), 150),
        (test_contract_path, test_contract.as_str(), 150),
        (test_execution_path, test_execution.as_str(), 350),
        (test_persistence_path, test_persistence.as_str(), 125),
    ] {
        assert_line_bound(owner_source, limit, owner);
    }
}

fn assert_admission_boundaries(adapter: &str, admission: &str) {
    for responsibility in [
        "pub(crate) struct AdmittedLaunch",
        "pub(crate) struct TeamMemberLaunch",
        "pub(crate) struct AdmittedTeamMember",
        "pub(crate) fn admit_team_members(",
        "pub(crate) fn resume_admitted_team_member(",
        "pub(super) fn admit_launch(",
        "fn recover_or_block_existing_child(",
    ] {
        assert_moved(admission, adapter, responsibility);
    }
}

fn assert_lifecycle_and_reporting_boundaries(adapter: &str, lifecycle: &str, reporting: &str) {
    for responsibility in [
        "pub fn cancel_report(",
        "pub(super) fn append_lifecycle_event(",
    ] {
        assert_moved(lifecycle, adapter, responsibility);
    }
    for responsibility in [
        "pub fn launch_report(",
        "pub fn status_report(",
        "pub(super) fn render_status_report(",
    ] {
        assert_moved(reporting, adapter, responsibility);
    }
}

fn assert_execution_boundaries(
    adapter: &str,
    execution: &str,
    completion: &str,
    dispatch: &str,
    member: &str,
    parent_merge: &str,
) {
    for module in [
        "mod completion;",
        "mod dispatch;",
        "mod member;",
        "mod parent_merge;",
    ] {
        assert_registered(execution, module, "subagent execution facade");
    }
    for responsibility in [
        "pub(crate) struct WorkerGeneration",
        "pub(crate) struct PreparedTeamMember",
        "pub(crate) struct CompletedTeamMember",
    ] {
        assert_moved(execution, adapter, responsibility);
    }
    for (owner, responsibility) in [
        (
            member,
            "pub(crate) fn terminalize_interrupted_team_members(",
        ),
        (member, "pub(crate) fn execute_admitted_team_member_with("),
        (member, "pub(crate) fn prepare_team_members("),
        (member, "pub(crate) fn execute_prepared_team_member_with("),
        (dispatch, "fn execute_prepared_launch("),
        (dispatch, "fn prepare_running("),
        (completion, "fn complete_generation("),
        (completion, "fn terminalize_locked("),
        (parent_merge, "fn merge_completed_result("),
        (parent_merge, "fn recover_completed_parent_merges("),
    ] {
        assert_moved(owner, execution, responsibility);
    }
}

fn assert_persistence_boundaries(adapter: &str, persistence: &str) {
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
        assert_moved(persistence, adapter, responsibility);
    }
}

fn assert_regression_boundaries(
    tests: &str,
    admission: &str,
    contract: &str,
    execution: &str,
    persistence: &str,
) {
    for module in [
        "#[path = \"tests/admission.rs\"]",
        "#[path = \"tests/contract.rs\"]",
        "#[path = \"tests/execution.rs\"]",
        "#[path = \"tests/persistence.rs\"]",
    ] {
        assert_registered(tests, module, "subagent test facade");
    }
    for (owner, regression) in [
        (
            contract,
            "fn launch_contract_enforces_role_tool_and_write_boundaries(",
        ),
        (
            persistence,
            "fn canonical_state_round_trips_and_preserves_hash_chain(",
        ),
        (execution, "fn dispatch_completes_and_merges_evidence_once("),
        (
            admission,
            "fn stale_running_child_recovers_as_failed_without_backend_replay(",
        ),
    ] {
        assert_moved(owner, tests, regression);
    }
}

fn assert_facade_delegation(adapter: &str) {
    for moved_definition in [
        "pub enum SubagentRole",
        "pub struct SubagentRecordV1",
        "fn validate_record",
        "fn render_record",
        "fn normalize_paths",
    ] {
        let production = adapter.split("#[cfg(test)]").next().unwrap_or(adapter);
        assert!(
            !production.contains(moved_definition),
            "subagent facade retains moved rule: {moved_definition}"
        );
    }
    assert!(
        adapter.contains("collaboration::subagent::*"),
        "subagent facade is missing domain delegation"
    );

    let result_adapter_path = "src/app/collaboration_adapter/subagent_result.rs";
    let result_adapter = source(result_adapter_path);
    let result_storage_path = "src/app/collaboration_adapter/subagent_result/storage.rs";
    let result_tests_path = "src/app/collaboration_adapter/subagent_result/tests.rs";
    let result_types_path = "src/app/collaboration_adapter/subagent_result/types.rs";
    let result_validation_path = "src/app/collaboration_adapter/subagent_result/validation.rs";
    let result_verification_path = "src/app/collaboration_adapter/subagent_result/verification.rs";
    let result_storage = source(result_storage_path);
    let result_tests = source(result_tests_path);
    let result_types = source(result_types_path);
    let result_validation = source(result_validation_path);
    let result_verification = source(result_verification_path);
    for registration in [
        "#[path = \"subagent_result/storage.rs\"]",
        "#[path = \"subagent_result/tests.rs\"]",
        "#[path = \"subagent_result/types.rs\"]",
        "#[path = \"subagent_result/validation.rs\"]",
        "#[path = \"subagent_result/verification.rs\"]",
    ] {
        assert!(
            result_adapter.contains(registration),
            "subagent result facade is missing owner registration: {registration}"
        );
    }
    for moved_definition in [
        "const RESULT_KEYS",
        "const EVIDENCE_V2_KEYS",
        "fn validate_patch",
        "fn verify_evidence_artifact",
        "fn render_evidence_payload_v2",
        "fn validate_bounded_text",
    ] {
        let production = result_adapter
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(&result_adapter);
        assert!(
            !production.contains(moved_definition),
            "subagent result facade retains moved rule: {moved_definition}"
        );
    }
    assert!(
        result_validation.contains("result_policy::parse_result_shape"),
        "subagent result validation owner is missing policy delegation"
    );
    for (owner, source, responsibility) in [
        (
            result_storage_path,
            &result_storage,
            "pub fn parse_and_store(",
        ),
        (
            result_types_path,
            &result_types,
            "pub struct StoredSubagentResult",
        ),
        (
            result_validation_path,
            &result_validation,
            "pub(super) fn parse_result_shape(",
        ),
        (
            result_verification_path,
            &result_verification,
            "pub fn verify_completed_artifacts(",
        ),
    ] {
        assert!(
            source.contains(responsibility),
            "subagent result owner {owner} is missing {responsibility}"
        );
        assert!(
            !result_adapter.contains(responsibility),
            "subagent result facade still owns {responsibility}"
        );
    }
    for (owner, source, line_budget) in [
        (result_adapter_path, &result_adapter, 50),
        (result_storage_path, &result_storage, 100),
        (result_tests_path, &result_tests, 225),
        (result_types_path, &result_types, 50),
        (result_validation_path, &result_validation, 100),
        (result_verification_path, &result_verification, 175),
    ] {
        assert!(
            source.lines().count() < line_budget,
            "subagent result owner {owner} exceeded its {line_budget}-line budget"
        );
    }
}
