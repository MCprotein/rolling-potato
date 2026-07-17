use super::*;

fn initialize_parent() -> state::WorkflowRecord {
    fs::create_dir_all(paths::project_root().join("src")).unwrap();
    fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
    state::initialize().unwrap();
    state::create_workflow("team parent fixture").unwrap()
}

fn manifest(parent: &state::WorkflowRecord, duplicate_write: bool) -> String {
    let (second_role, second_tools, second_write) = if duplicate_write {
        (
            "executor",
            "[\"read_file\",\"render_diff\"]",
            "[\"src/main.rs\"]",
        )
    } else {
        ("verifier", "[\"read_file\"]", "[]")
    };
    format!(
        "{{\"schema_version\":1,\"team_id\":\"team-fixture\",\"parent_workflow_id\":\"{}\",\"members\":[{{\"lane\":1,\"id\":\"executor-1\",\"role\":\"executor\",\"task\":\"prepare a bounded diff\",\"tools\":[\"read_file\",\"render_diff\"],\"read_paths\":[\"src/main.rs\"],\"write_paths\":[\"src/main.rs\"],\"timeout_ms\":30000,\"max_tokens\":256}},{{\"lane\":2,\"id\":\"verifier-1\",\"role\":\"{}\",\"task\":\"verify the bounded result\",\"tools\":{},\"read_paths\":[\"src/main.rs\"],\"write_paths\":{},\"timeout_ms\":30000,\"max_tokens\":256}}],\"write_policy\":\"single_writer\",\"merge_policy\":\"runtime_owned\",\"stop_gate\":\"evidence_required\"}}",
        parent.workflow_id, second_role, second_tools, second_write,
    )
}

#[test]
fn plan_persists_canonical_manifest_and_hash_chained_state() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let body = manifest(&parent, false);
    fs::write(paths::project_root().join("team.json"), &body).unwrap();

    let report = plan_report("team.json").unwrap();
    let record = load_state("team-fixture").unwrap();
    let latest = latest_for_parent(&parent.workflow_id).unwrap().unwrap();
    let retry = plan_report("team.json").unwrap();
    let status = super::super::team::status_report().unwrap();
    let planned_events = ledger::read_runtime_events()
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == "team.stage.planned")
        .count();

    assert!(report.contains("status: planned"));
    assert!(retry.contains("status: planned"));
    assert_eq!(planned_events, 1);
    assert!(status.contains("current team id: team-fixture"));
    assert!(status.contains("current team stage: team-plan"));
    assert!(report.contains("stage: team-plan"));
    assert_eq!(record.revision, 1);
    assert_eq!(record.previous_hash, "none");
    assert_eq!(record.manifest_hash, state::sha256_text(&body));
    assert_eq!(record, latest);
    assert_eq!(
        state::read_regular_file_bounded(
            &paths::project_team_manifest_file("team-fixture"),
            MAX_MANIFEST_BYTES,
            "test manifest",
        )
        .unwrap(),
        body
    );
}

#[test]
fn manifest_rejects_parent_mismatch_and_cross_lane_ownership() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let mismatch = manifest(&parent, false).replace(&parent.workflow_id, "workflow-other");
    fs::write(paths::project_root().join("mismatch.json"), mismatch).unwrap();
    assert!(plan_report("mismatch.json")
        .unwrap_err()
        .message
        .contains("parent binding"));

    let conflict = manifest(&parent, true);
    assert!(parse_manifest(&conflict)
        .unwrap_err()
        .message
        .contains("ownership 충돌"));

    let ancestor_conflict = conflict.replacen(
        "\"write_paths\":[\"src/main.rs\"]",
        "\"write_paths\":[\"src\"]",
        1,
    );
    assert!(parse_manifest(&ancestor_conflict)
        .unwrap_err()
        .message
        .contains("ownership 충돌"));
}

#[test]
fn stage_machine_allows_only_ordered_runtime_transitions() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let body = manifest(&parent, false);
    fs::write(paths::project_root().join("team.json"), body).unwrap();
    plan_report("team.json").unwrap();

    let planned = load_state("team-fixture").unwrap();
    let mut invalid = planned.clone();
    assert!(invalid
        .transition_to(TeamStage::Execute, None, None)
        .unwrap_err()
        .message
        .contains("stage 전이 차단"));

    let mut dispatched = planned.clone();
    dispatched
        .transition_to(TeamStage::Dispatch, Some(2), Some("parallel"))
        .unwrap();
    let dispatched = checkpoint_state(dispatched, planned.revision).unwrap();
    let mut executing = dispatched.clone();
    executing
        .transition_to(TeamStage::Execute, None, None)
        .unwrap();
    let executing = checkpoint_state(executing, dispatched.revision).unwrap();

    assert_eq!(executing.stage, TeamStage::Execute);
    assert_eq!(executing.revision, 3);
    assert_eq!(executing.admitted_lanes, 2);
    assert_eq!(executing.execution_mode, "parallel");
    assert_eq!(load_state("team-fixture").unwrap(), executing);
}

#[test]
fn cancellation_marker_is_durable_idempotent_and_hash_bound() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    let body = manifest(&parent, false);
    fs::write(paths::project_root().join("team.json"), body).unwrap();
    plan_report("team.json").unwrap();

    let report = cancel_report("team-fixture").unwrap();
    let retry = cancel_report("team-fixture").unwrap();
    let cancelled = load_state("team-fixture").unwrap();

    assert!(report.contains("status: cancellation-requested"));
    assert!(retry.contains("status: already-cancelled"));
    assert_eq!(cancelled.stage, TeamStage::Cancelled);
    assert!(cancellation_requested("team-fixture").unwrap());

    let marker_path = paths::project_team_cancel_file("team-fixture");
    let marker = fs::read_to_string(&marker_path).unwrap();
    fs::write(
        &marker_path,
        marker.replace(&cancelled.manifest_hash, &"0".repeat(64)),
    )
    .unwrap();
    assert!(cancellation_requested("team-fixture")
        .unwrap_err()
        .message
        .contains("immutable binding"));
}

#[test]
fn tampered_current_state_is_rejected_against_artifact_hash() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let parent = initialize_parent();
    fs::write(
        paths::project_root().join("team.json"),
        manifest(&parent, false),
    )
    .unwrap();
    plan_report("team.json").unwrap();

    let path = paths::project_team_file("team-fixture");
    let tampered = fs::read_to_string(&path)
        .unwrap()
        .replace("\"status\":\"active\"", "\"status\":\"failed\"");
    fs::write(path, tampered).unwrap();

    assert!(load_state("team-fixture")
        .unwrap_err()
        .message
        .contains("status/stage binding"));
}
