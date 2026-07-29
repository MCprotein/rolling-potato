use std::fs;

use super::*;
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::{ledger, state};

#[test]
fn rejects_remote_artifact_pointer() {
    let err = validate_artifact_pointer("https://example.com/evidence.json")
        .expect_err("remote evidence pointers must be blocked");
    assert_eq!(err.code, 3);
}

#[test]
fn rejects_parent_dir_artifact_pointer() {
    let err = validate_artifact_pointer("../outside.log")
        .expect_err("parent directory evidence pointers must be blocked");
    assert_eq!(err.code, 3);
}

#[test]
fn validate_report_preserves_boundary_summary() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-evidence-report-test-{}",
        std::process::id()
    ));
    let project = root.join("project");
    let artifact = project.join(".rpotato/evidence/one.json");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    fs::write(&artifact, "{}\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);

    let report = validate_report(".rpotato/evidence/one.json").unwrap();
    let canonical_project = fs::canonicalize(&project).unwrap();
    let canonical_artifact = fs::canonicalize(&artifact).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = fs::remove_dir_all(root);

    assert_eq!(
        report,
        format!(
            "evidence validate 결과\n- artifact: {}\n- project root: {}\n- boundary: project root 내부\n- stale policy: {}\n- 동작: artifact pointer가 존재하고 project boundary를 벗어나지 않는지 확인했습니다.",
            canonical_artifact.display(),
            canonical_project.display(),
            stale_policy_summary()
        )
    );
}

#[test]
fn store_status_counts_runtime_records_and_project_artifacts() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-evidence-store-test-{}",
        std::process::id()
    ));
    let project = root.join("project");
    let data = root.join("data");
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);

    fs::create_dir_all(paths::state_dir()).unwrap();
    fs::create_dir_all(paths::project_evidence_dir().join("nested")).unwrap();
    fs::write(
        paths::runtime_evidence_file(),
        "{\"evidence_id\":\"one\"}\n\n{\"evidence_id\":\"two\"}\n",
    )
    .unwrap();
    fs::write(paths::project_evidence_dir().join("one.txt"), "one").unwrap();
    fs::write(
        paths::project_evidence_dir().join("nested").join("two.txt"),
        "two",
    )
    .unwrap();

    let status = store_status().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");

    assert_eq!(status.runtime_evidence_records, 2);
    assert_eq!(status.project_artifacts, 2);
    assert_eq!(status.stale_policy, stale_policy_summary());
}

#[test]
fn bounded_store_status_reports_scan_truncation_and_rejects_zero_budget() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-evidence-bounded-store-test-{}",
        std::process::id()
    ));
    let project = root.join("project");
    let data = root.join("data");
    let _ = fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);

    fs::create_dir_all(paths::state_dir()).unwrap();
    fs::create_dir_all(paths::project_evidence_dir()).unwrap();
    fs::write(paths::runtime_evidence_file(), "{}\n{}\n").unwrap();
    fs::write(paths::project_evidence_dir().join("one.json"), "one").unwrap();
    fs::write(paths::project_evidence_dir().join("two.json"), "two").unwrap();

    let status = store_status_bounded(1, 1_024).unwrap();
    assert_eq!(status.runtime_evidence_records, 1);
    assert_eq!(status.project_artifacts, 1);
    assert!(status.truncated);
    assert!(store_status_bounded(0, 1_024).is_err());
    assert!(store_status_bounded(1, 0).is_err());

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stop_gate_rejects_missing_and_stale_evidence() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("rpotato-stop-gate-test-{}", std::process::id()));
    let project = root.join("project");
    let data = root.join("data");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(project.join(".rpotato/evidence")).unwrap();
    fs::write(project.join("src/lib.rs"), "after\n").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", &data);
    let after_hash = state::sha256_text("after\n");
    let mut workflow = state::WorkflowRecord::new(&ledger::fresh_identity(), "test");
    workflow.phase = "verified".to_string();
    workflow.approval_state = "approved".to_string();
    workflow.proposal_id = "patch-proposal-test".to_string();
    workflow.source_path = "src/lib.rs".to_string();
    workflow.after_hash = after_hash.clone();
    workflow.evidence_id = "evidence-missing".to_string();
    workflow.evidence_hash = "expected".to_string();

    let missing = evaluate_patch_stop_gate(&workflow).unwrap_err();
    assert_eq!(missing.code, 3);

    fs::write(
        project.join(".rpotato/evidence/evidence-missing.json"),
        format!(
            "{{\"artifact_hash\": \"wrong\", \"workflow_id\": \"{}\", \"proposal_id\": \"{}\", \"source_hash\": \"{}\", \"passed\": true}}",
            workflow.workflow_id, workflow.proposal_id, after_hash
        ),
    )
    .unwrap();
    let stale = evaluate_patch_stop_gate(&workflow).unwrap_err();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
    assert_eq!(stale.code, 3);
    assert!(stale.message.contains("malformed verification evidence"));
}

#[test]
fn evidence_crash_after_event_is_idempotent_without_duplicate_receipts() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    for point in ["after-artifact", "after-runtime", "after-event"] {
        let root = std::env::temp_dir().join(format!(
            "rpotato-evidence-dedupe-{point}-{}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(project.join("src")).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        state::initialize().unwrap();
        let mut workflow = state::WorkflowRecord::new(&ledger::fresh_identity(), "evidence dedupe");
        workflow.proposal_id = "patch-proposal-evidence-test".to_string();
        workflow.action_id = "action-evidence-test".to_string();
        let source_hash = state::sha256_text("after\n");
        std::env::set_var("RPOTATO_TEST_EVIDENCE_FAULT", point);
        let injected =
            record_patch_verification(&workflow, "pwd", true, "0", &source_hash, "ok", "")
                .unwrap_err();
        std::env::remove_var("RPOTATO_TEST_EVIDENCE_FAULT");
        let receipt =
            record_patch_verification(&workflow, "pwd", true, "0", &source_hash, "ok", "").unwrap();
        let runtime_records = fs::read_to_string(paths::runtime_evidence_file())
            .unwrap()
            .lines()
            .filter(|line| line.contains(&receipt.evidence_id))
            .count();
        let ledger_events = ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| {
                event.event_type == "verification.evidence.recorded"
                    && event
                        .details
                        .contains(&format!("evidence_id={}", receipt.evidence_id))
            })
            .count();
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);
        assert_eq!(injected.code, 1, "point: {point}");
        assert_eq!(runtime_records, 1, "point: {point}");
        assert_eq!(ledger_events, 1, "point: {point}");
    }
}
