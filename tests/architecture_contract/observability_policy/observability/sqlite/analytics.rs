fn assert_sqlite_observability_analytics() {
    let facade_path = "src/adapters/sqlite/observability_projection/analytics.rs";
    let latest_model_run_path =
        "src/adapters/sqlite/observability_projection/analytics/latest_model_run.rs";
    let latest_model_run_tests_path =
        "src/adapters/sqlite/observability_projection/analytics/latest_model_run/tests.rs";
    let model_summaries_path =
        "src/adapters/sqlite/observability_projection/analytics/model_summaries.rs";
    let optimization_policy_path =
        "src/adapters/sqlite/observability_projection/analytics/optimization_policy.rs";
    let performance_baseline_path =
        "src/adapters/sqlite/observability_projection/analytics/performance_baseline.rs";
    let statistics_path =
        "src/adapters/sqlite/observability_projection/analytics/statistics.rs";
    for path in [
        facade_path,
        latest_model_run_path,
        latest_model_run_tests_path,
        model_summaries_path,
        optimization_policy_path,
        performance_baseline_path,
        statistics_path,
    ] {
        assert!(Path::new(path).is_file(), "missing analytics owner: {path}");
    }

    let facade = fs::read_to_string(facade_path).unwrap();
    let latest_model_run = fs::read_to_string(latest_model_run_path).unwrap();
    let latest_model_run_tests = fs::read_to_string(latest_model_run_tests_path).unwrap();
    let model_summaries = fs::read_to_string(model_summaries_path).unwrap();
    let optimization_policy = fs::read_to_string(optimization_policy_path).unwrap();
    let performance_baseline = fs::read_to_string(performance_baseline_path).unwrap();
    let statistics = fs::read_to_string(statistics_path).unwrap();

    for owner in [
        "latest_model_run",
        "model_summaries",
        "optimization_policy",
        "performance_baseline",
        "statistics",
    ] {
        assert!(
            facade
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "SQLite analytics does not register its {owner} owner"
        );
    }
    assert!(latest_model_run.contains("fn latest_model_run_for_session_from_connection("));
    assert!(latest_model_run_tests
        .contains("fn latest_model_run_is_scoped_to_the_requested_session("));

    for (owner, responsibility) in [
        (&model_summaries, "fn model_summaries_from_connection("),
        (&model_summaries, "fn model_summaries("),
        (&performance_baseline, "fn performance_baseline("),
        (&performance_baseline, "fn query_baseline_model_rows("),
        (&optimization_policy, "fn optimization_policy("),
        (&optimization_policy, "fn benchmark_evidence_summary("),
        (&statistics, "fn percentile("),
    ] {
        assert!(
            owner.contains(responsibility),
            "SQLite analytics owner is missing: {responsibility}"
        );
        assert!(
            !facade.contains(responsibility),
            "SQLite analytics facade owns behavior: {responsibility}"
        );
    }

    for (owner, maximum_lines) in [
        (&facade, 30),
        (&latest_model_run, 100),
        (&latest_model_run_tests, 75),
        (&model_summaries, 100),
        (&optimization_policy, 100),
        (&performance_baseline, 275),
        (&statistics, 40),
    ] {
        assert!(
            owner.lines().count() < maximum_lines,
            "SQLite analytics owner regrew beyond {maximum_lines} lines"
        );
    }
}
