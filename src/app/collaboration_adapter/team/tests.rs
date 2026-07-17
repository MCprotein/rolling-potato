use super::*;
use std::fs;

#[test]
fn status_falls_back_to_sequential_when_resource_sample_is_missing() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-status-no-sample-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    let report = status_report().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert!(report.contains("team status"));
    assert!(report.contains("resource sample source: no-sample"));
    assert!(report.contains("resource pressure: unknown"));
    assert!(report.contains("admission: sequential-fallback"));
    assert!(report.contains("admitted lanes: 1"));
    assert!(report.contains("boundary: read-only"));
}

#[test]
fn status_blocks_team_lanes_on_critical_resource_sample() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-status-critical-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    observability::record_resource_sample(&observability::ResourceSampleMetric {
        resource_sample_id: "resource-sample-team-critical".to_string(),
        session_id: "session-team-critical".to_string(),
        backend_id: "llama.cpp".to_string(),
        pid: 4242,
        process_cpu_percent: Some(98.0),
        average_rss_bytes: Some(14 * 1024 * 1024 * 1024),
        peak_rss_bytes: Some(14 * 1024 * 1024 * 1024),
        disk_bytes: Some(2048),
        sample_count: 1,
        pressure_status: "critical".to_string(),
        recorded_at_ms: 1234,
    })
    .unwrap();

    let report = status_report().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert!(report.contains("resource sample source: latest-resource-sample"));
    assert!(report.contains("resource pressure: critical"));
    assert!(report.contains("resource cpu percent: 98.0"));
    assert!(report.contains("admission: blocked"));
    assert!(report.contains("dispatch blocked: yes"));
    assert!(report.contains("admitted lanes: 0"));
    assert!(report.contains("fallback: wait"));
}

#[test]
fn admission_allows_parallel_and_records_ledger_event() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-admission-normal-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample("resource-sample-team-normal", "normal", Some(17.0));

    let report = admission_report(3, &[], &[], &["cargo test".to_string()]).unwrap();
    let store = observability::status().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert!(report.contains("team admission"));
    assert!(report.contains("status: admitted"));
    assert!(report.contains("requested parallel lanes: 3"));
    assert!(report.contains("admitted lanes: 3"));
    assert!(report.contains("admission: allow-parallel"));
    assert!(report.contains("dispatch blocked: no"));
    assert!(report.contains("policy checks: 1"));
    assert!(report.contains("policy status: allowed"));
    assert!(report.contains("command: cargo test -> allow"));
    assert!(report.contains("ownership claims: 0"));
    assert!(report.contains("ownership status: not-requested"));
    assert!(report.contains("approval request: not-required"));
    assert!(report.contains("ledger event: event-"));
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn admission_blocks_critical_pressure_and_records_ledger_event() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-admission-critical-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample(
        "resource-sample-team-admit-critical",
        "critical",
        Some(98.0),
    );

    let err = admission_report(4, &[], &[], &[]).unwrap_err();
    let store = observability::status().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 3);
    assert!(err.message.contains("team admission 차단"));
    assert!(err.message.contains("status: blocked"));
    assert!(err.message.contains("resource pressure: critical"));
    assert!(err.message.contains("requested parallel lanes: 4"));
    assert!(err.message.contains("admitted lanes: 0"));
    assert!(err.message.contains("dispatch blocked: yes"));
    assert!(err.message.contains("approval request: not-required"));
    assert!(err.message.contains("ledger event: event-"));
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn admission_blocks_write_policy_preflight_and_records_ledger_event() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-admission-policy-write-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample("resource-sample-team-policy-write", "normal", Some(17.0));

    let err = admission_report(2, &["README.md".to_string()], &[], &[]).unwrap_err();
    let store = observability::status().unwrap();
    let approval_requests = approval::request_summaries(5).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 3);
    assert!(err.message.contains("status: policy-blocked"));
    assert!(err.message.contains("admission: allow-parallel"));
    assert!(err.message.contains("policy checks: 1"));
    assert!(err.message.contains("policy status: approval-required"));
    assert!(err.message.contains("write: README.md -> ask"));
    assert!(err.message.contains("dispatch blocked: yes"));
    assert!(err.message.contains("approval request: team-event-"));
    assert!(err.message.contains("approval request path:"));
    assert!(err.message.contains("ledger event: event-"));
    assert_eq!(approval_requests.len(), 1);
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn admission_reports_file_ownership_allocation() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-admission-ownership-test");
    let project_root = root.join("project");
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::write(project_root.join("src/app.rs"), "").unwrap();
    fs::write(project_root.join("src/cli.rs"), "").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample("resource-sample-team-ownership", "normal", Some(17.0));

    let err = admission_report(
        2,
        &[],
        &[(1, "src/app.rs".to_string()), (2, "src/cli.rs".to_string())],
        &[],
    )
    .unwrap_err();
    let store = observability::status().unwrap();
    let approval_requests = approval::request_summaries(5).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 3);
    assert!(err.message.contains("status: policy-blocked"));
    assert!(err.message.contains("policy checks: 2"));
    assert!(err.message.contains("policy status: approval-required"));
    assert!(err.message.contains("ownership claims: 2"));
    assert!(err.message.contains("ownership status: allocated"));
    assert!(err.message.contains("ownership blocked: no"));
    assert!(err
        .message
        .contains("lane 1: src/app.rs -> assigned (normalized: src/app.rs"));
    assert!(err
        .message
        .contains("lane 2: src/cli.rs -> assigned (normalized: src/cli.rs"));
    assert!(err.message.contains("approval request: team-event-"));
    assert_eq!(approval_requests.len(), 1);
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn admission_blocks_cross_lane_file_ownership_conflict() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-admission-ownership-conflict-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    fs::write(project_root.join("README.md"), "").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample(
        "resource-sample-team-ownership-conflict",
        "normal",
        Some(17.0),
    );

    let err = admission_report(
        2,
        &[],
        &[(1, "README.md".to_string()), (2, "./README.md".to_string())],
        &[],
    )
    .unwrap_err();
    let store = observability::status().unwrap();
    let approval_requests = approval::request_summaries(5).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 3);
    assert!(err.message.contains("status: ownership-blocked"));
    assert!(err.message.contains("policy status: approval-required"));
    assert!(err.message.contains("ownership status: conflict"));
    assert!(err.message.contains("ownership blocked: yes"));
    assert!(err
        .message
        .contains("lane 2: ./README.md -> conflict (normalized: README.md"));
    assert!(err.message.contains("approval request: team-event-"));
    assert_eq!(approval_requests.len(), 1);
    assert!(err.message.contains("ledger event: event-"));
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn dispatch_enforces_file_ownership_at_dispatch_time() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-dispatch-ownership-test");
    let project_root = root.join("project");
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::write(project_root.join("src/team.rs"), "").unwrap();
    fs::write(project_root.join("src/cli.rs"), "").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample("resource-sample-team-dispatch-normal", "normal", Some(17.0));

    let report = dispatch_report(
        2,
        &[
            (1, "src/team.rs".to_string()),
            (2, "src/cli.rs".to_string()),
        ],
        None,
        None,
    )
    .unwrap();
    let store = observability::status().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert!(report.contains("team dispatch"));
    assert!(report.contains("status: admitted"));
    assert!(report.contains("lane admission: allow-parallel"));
    assert!(report.contains("dispatch blocked: no"));
    assert!(report.contains("ownership claims: 2"));
    assert!(report.contains("ownership status: allocated"));
    assert!(report.contains("ownership blocked: no"));
    assert!(report.contains("continuation status: not-requested"));
    assert!(report.contains("ledger event: event-"));
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn dispatch_blocks_cross_lane_file_ownership_conflict() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-dispatch-ownership-conflict-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    fs::write(project_root.join("README.md"), "").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample(
        "resource-sample-team-dispatch-conflict",
        "normal",
        Some(17.0),
    );

    let err = dispatch_report(
        2,
        &[(1, "README.md".to_string()), (2, "./README.md".to_string())],
        None,
        None,
    )
    .unwrap_err();
    let store = observability::status().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 3);
    assert!(err.message.contains("team dispatch 차단"));
    assert!(err.message.contains("status: ownership-blocked"));
    assert!(err.message.contains("ownership status: conflict"));
    assert!(err.message.contains("ownership blocked: yes"));
    assert!(err
        .message
        .contains("lane 2: ./README.md -> conflict (normalized: README.md"));
    assert!(err.message.contains("ledger event: event-"));
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn dispatch_records_failed_worker_continuation_and_status_surfaces_it() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-dispatch-continuation-test");
    let project_root = root.join("project");
    fs::create_dir_all(project_root.join("src")).unwrap();
    fs::write(project_root.join("src/team.rs"), "").unwrap();
    fs::write(project_root.join("src/cli.rs"), "").unwrap();
    fs::write(project_root.join("src/app.rs"), "").unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample(
        "resource-sample-team-dispatch-continuation",
        "normal",
        Some(17.0),
    );

    let dispatch = dispatch_report(
        3,
        &[
            (1, "src/team.rs".to_string()),
            (2, "src/cli.rs".to_string()),
            (3, "src/app.rs".to_string()),
        ],
        Some(2),
        Some("worker timed out"),
    )
    .unwrap();
    let status = status_report().unwrap();
    let store = observability::status().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert!(dispatch.contains("status: continuation-ready"));
    assert!(dispatch.contains("failed lane: 2"));
    assert!(dispatch.contains("failure reason: worker timed out"));
    assert!(dispatch.contains("continuation status: continue-with-remaining"));
    assert!(dispatch.contains("continuation action: continue"));
    assert!(dispatch.contains("continuation remaining lanes: 2"));
    assert!(status.contains("latest team runtime event: team.continuation.recorded"));
    assert!(status.contains("latest team runtime summary: team continuation recorded"));
    assert!(status.contains("latest team runtime event id: event-"));
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn governor_clamps_context_and_records_ledger_event() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-governor-clamp-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample("resource-sample-team-governor-normal", "normal", Some(17.0));

    let report = governor_report(2, 6000, Some(4096), resource::ModelTier::Standard).unwrap();
    let store = observability::status().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert!(report.contains("team governor"));
    assert!(report.contains("status: clamped"));
    assert!(report.contains("requested context tokens: 6000"));
    assert!(report.contains("context limit tokens: 4096"));
    assert!(report.contains("effective context tokens: 4096"));
    assert!(report.contains("context action: clamped"));
    assert!(report.contains("model route hint: escalate"));
    assert!(report.contains("dispatch blocked: no"));
    assert!(report.contains("ledger event: event-"));
    assert_eq!(store.ledger_events, 1);
}

#[test]
fn governor_blocks_critical_pressure_and_records_ledger_event() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-team-governor-critical-test");
    let project_root = root.join("project");
    fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

    record_resource_sample(
        "resource-sample-team-governor-critical",
        "critical",
        Some(98.0),
    );

    let err = governor_report(2, 1024, Some(4096), resource::ModelTier::Small).unwrap_err();
    let store = observability::status().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 3);
    assert!(err.message.contains("team governor 차단"));
    assert!(err.message.contains("status: blocked"));
    assert!(err.message.contains("resource pressure: critical"));
    assert!(err.message.contains("context action: blocked"));
    assert!(err.message.contains("model route hint: defer"));
    assert!(err.message.contains("dispatch blocked: yes"));
    assert!(err.message.contains("ledger event: event-"));
    assert_eq!(store.ledger_events, 1);
}

fn record_resource_sample(id: &str, pressure_status: &str, cpu: Option<f64>) {
    observability::record_resource_sample(&observability::ResourceSampleMetric {
        resource_sample_id: id.to_string(),
        session_id: format!("session-{id}"),
        backend_id: "llama.cpp".to_string(),
        pid: 4242,
        process_cpu_percent: cpu,
        average_rss_bytes: Some(512 * 1024 * 1024),
        peak_rss_bytes: Some(512 * 1024 * 1024),
        disk_bytes: Some(2048),
        sample_count: 1,
        pressure_status: pressure_status.to_string(),
        recorded_at_ms: 1234,
    })
    .unwrap();
}

fn test_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    root
}
