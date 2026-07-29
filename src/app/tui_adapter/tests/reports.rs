#[test]
fn approvals_renders_team_admission_request() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-tui-approvals-team-test");
    let project_root = root.join("project");
    std::fs::create_dir_all(&project_root).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    crate::app::workflow_adapter::state::initialize().unwrap();

    crate::app::observability_adapter::record_resource_sample(
        &crate::app::observability_adapter::ResourceSampleMetric {
            resource_sample_id: "resource-sample-tui-approvals-team".to_string(),
            session_id: "session-tui-approvals-team".to_string(),
            backend_id: "llama.cpp".to_string(),
            pid: 4242,
            process_cpu_percent: Some(12.0),
            average_rss_bytes: Some(512 * 1024 * 1024),
            peak_rss_bytes: Some(512 * 1024 * 1024),
            disk_bytes: Some(2048),
            sample_count: 1,
            pressure_status: "normal".to_string(),
            recorded_at_ms: 1234,
        },
    )
    .unwrap();
    let err = crate::app::collaboration_adapter::team::admission_report(
        2,
        &["README.md".to_string()],
        &[],
        &[],
    )
    .unwrap_err();
    let report = approvals_report().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");

    assert!(err.message.contains("approval request: team-event-"));
    assert!(report.contains("team-admission"));
    assert!(report.contains("pending-approval"), "{report}");
    assert!(report.contains("canonical-event="));
}

#[test]
fn evidence_renders_stop_gate_status_without_mutating() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-tui-evidence-test");
    let project_root = root.join("project");
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("COLUMNS", "68");

    std::fs::create_dir_all(paths::state_dir()).unwrap();
    std::fs::create_dir_all(paths::project_evidence_dir()).unwrap();
    std::fs::write(
        paths::runtime_evidence_file(),
        "{\"evidence_id\":\"one\"}\n",
    )
    .unwrap();
    std::fs::write(paths::project_evidence_dir().join("one.txt"), "one").unwrap();

    let report = evidence_report().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("COLUMNS");

    assert!(report.contains("rpotato TUI beta - evidence"));
    assert!(report.contains("mode: read-only evidence status"));
    assert!(report.contains("runtime records: 1"));
    assert!(report.contains("project artifacts: 1"));
    assert!(report.contains("[stop gate boundary]"));
    assert!(report.contains("terminal gate: not implemented"));
    assert!(report.contains("validate: rpotato evidence validate <artifact-pointer>"));
    assert!(report.contains("beta boundary"));
}
