pub(super) fn assert_sqlite_observability_projection() {
    let sqlite = fs::read_to_string("src/adapters/sqlite/observability_projection.rs").unwrap();
    let analytics_path = "src/adapters/sqlite/observability_projection/analytics.rs";
    let latest_model_run_path =
        "src/adapters/sqlite/observability_projection/analytics/latest_model_run.rs";
    let latest_model_run_tests_path =
        "src/adapters/sqlite/observability_projection/analytics/latest_model_run/tests.rs";
    let metrics_path = "src/adapters/sqlite/observability_projection/metrics.rs";
    let read_snapshot_path = "src/adapters/sqlite/observability_projection/read_snapshot.rs";
    let replay_path = "src/adapters/sqlite/observability_projection/replay.rs";
    let schema_path = "src/adapters/sqlite/observability_projection/schema.rs";
    let sessions_path = "src/adapters/sqlite/observability_projection/sessions.rs";
    let sqlite_tests_path = "src/adapters/sqlite/observability_projection/tests.rs";
    let sqlite_projection_tests_path =
        "src/adapters/sqlite/observability_projection/tests/projection.rs";
    let sqlite_recovery_tests_path =
        "src/adapters/sqlite/observability_projection/tests/recovery.rs";
    let sqlite_storage_tests_path = "src/adapters/sqlite/observability_projection/tests/storage.rs";
    assert!(Path::new(analytics_path).is_file());
    assert!(Path::new(latest_model_run_path).is_file());
    assert!(Path::new(latest_model_run_tests_path).is_file());
    assert!(Path::new(metrics_path).is_file());
    assert!(Path::new(read_snapshot_path).is_file());
    assert!(Path::new(replay_path).is_file());
    assert!(Path::new(schema_path).is_file());
    assert!(Path::new(sessions_path).is_file());
    assert!(Path::new(sqlite_tests_path).is_file());
    assert!(Path::new(sqlite_projection_tests_path).is_file());
    assert!(Path::new(sqlite_recovery_tests_path).is_file());
    assert!(Path::new(sqlite_storage_tests_path).is_file());
    let analytics = fs::read_to_string(analytics_path).unwrap();
    let latest_model_run = fs::read_to_string(latest_model_run_path).unwrap();
    let latest_model_run_tests = fs::read_to_string(latest_model_run_tests_path).unwrap();
    let metrics = fs::read_to_string(metrics_path).unwrap();
    let read_snapshot = fs::read_to_string(read_snapshot_path).unwrap();
    let replay = fs::read_to_string(replay_path).unwrap();
    let schema = fs::read_to_string(schema_path).unwrap();
    let sessions = fs::read_to_string(sessions_path).unwrap();
    let sqlite_tests = fs::read_to_string(sqlite_tests_path).unwrap();
    let sqlite_projection_tests = fs::read_to_string(sqlite_projection_tests_path).unwrap();
    let sqlite_recovery_tests = fs::read_to_string(sqlite_recovery_tests_path).unwrap();
    let sqlite_storage_tests = fs::read_to_string(sqlite_storage_tests_path).unwrap();
    let projection_port_impl = "impl ObservabilityProjectionPort for SqliteObservabilityProjection";
    assert!(
        sqlite.contains(projection_port_impl),
        "SQLite adapter is missing: {projection_port_impl}"
    );
    assert!(
        replay.contains("pub(super) fn replay_ledger_events("),
        "SQLite replay owner is missing canonical replay"
    );
    assert!(
        schema.contains("PRAGMA journal_mode = WAL"),
        "SQLite schema owner is missing WAL migration policy"
    );
    let sqlite_production = sqlite.split("#[cfg(test)]").next().unwrap_or(&sqlite);
    assert!(
        !sqlite_production.contains("crate::ledger"),
        "SQLite projection adapter bypasses the consumer-owned projection port"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod analytics;"),
        "SQLite projection does not register the analytics owner"
    );
    assert!(
        analytics
            .lines()
            .any(|line| line == "mod latest_model_run;"),
        "SQLite analytics does not register its latest-model-run owner"
    );
    assert!(
        latest_model_run.contains(
            "pub(in crate::adapters::sqlite::observability_projection) fn latest_model_run_for_session_from_connection("
        ),
        "latest-model-run query owner is missing its session-scoped query"
    );
    assert!(
        !analytics.contains("pub(super) fn latest_model_run_for_session_from_connection("),
        "SQLite analytics still owns the session-scoped latest-model-run query"
    );
    assert!(
        latest_model_run_tests.contains("fn latest_model_run_is_scoped_to_the_requested_session(")
    );
    assert!(
        sqlite.lines().any(|line| line == "mod metrics;"),
        "SQLite projection does not register the metric owner"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod read_snapshot;"),
        "SQLite projection does not register the read-only snapshot owner"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod replay;"),
        "SQLite projection does not register the replay owner"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod schema;"),
        "SQLite projection does not register the schema owner"
    );
    assert!(
        sqlite.lines().any(|line| line == "mod sessions;"),
        "SQLite projection does not register the session query owner"
    );
    for responsibility in ["pub(super) fn migrate(", "fn ensure_column("] {
        assert!(
            !sqlite.contains(responsibility),
            "schema responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            schema.contains(responsibility),
            "SQLite schema owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn model_summaries_from_connection(",
        "pub(super) fn model_summaries(",
        "pub(super) fn performance_baseline(",
        "pub(super) fn optimization_policy(",
        "fn query_baseline_model_rows(",
        "fn benchmark_evidence_summary(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "analytics responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            analytics.contains(responsibility),
            "SQLite analytics owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn record_model_run(",
        "pub(super) fn record_resource_sample(",
        "pub(super) fn record_benchmark_run(",
        "pub(super) fn benchmark_run_reports(",
        "pub(super) fn latest_resource_sample(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "metric responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            metrics.contains(responsibility),
            "SQLite metric owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn record_session(",
        "pub(super) fn replay_ledger_events(",
        "pub(super) fn project_sessions_from_events(",
        "pub(super) fn insert_ledger_event(",
        "pub(super) fn project_workflow_checkpoint(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "replay responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            replay.contains(responsibility),
            "SQLite replay owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn session_history(",
        "pub fn session_entry(",
        "pub fn session_events(",
        "fn query_session_history(",
        "fn query_session_events(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "session query responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            sessions.contains(responsibility),
            "SQLite session query owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) struct ReadOnlyProjection",
        "pub(super) fn open_read_only(",
        "pub(super) fn open_read_only_path(",
        "fn stable_projection_files(",
        "fn read_regular_snapshot_file(",
        "fn write_private_snapshot_file(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "read-only snapshot responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            read_snapshot.contains(responsibility),
            "read-only snapshot owner is missing: {responsibility}"
        );
    }
    assert!(
        sqlite.contains("#[path = \"observability_projection/tests.rs\"]"),
        "SQLite projection does not register its regression-test owner"
    );
    for include in [
        "include!(\"tests/recovery.rs\");",
        "include!(\"tests/projection.rs\");",
        "include!(\"tests/storage.rs\");",
    ] {
        assert!(
            sqlite_tests.contains(include),
            "SQLite projection regression facade is missing: {include}"
        );
    }
    for (owner, responsibility) in [
        (
            &sqlite_recovery_tests,
            "fn corrupt_sqlite_is_preserved_before_canonical_ledger_failure(",
        ),
        (
            &sqlite_recovery_tests,
            "fn sqlite_replay_faults_are_atomic_and_concurrent_readers_see_complete_rows(",
        ),
        (
            &sqlite_storage_tests,
            "fn performance_baseline_aggregates_local_metrics(",
        ),
        (
            &sqlite_storage_tests,
            "fn optimization_policy_reads_metrics_and_measured_benchmark_evidence(",
        ),
        (
            &sqlite_projection_tests,
            "fn supplied_event_ordinal_avoids_a_canonical_ledger_rescan(",
        ),
    ] {
        assert!(
            owner.contains(responsibility),
            "SQLite projection regression owner is missing: {responsibility}"
        );
    }
    for moved_responsibility in [
        "fn corrupt_sqlite_is_preserved_before_canonical_ledger_failure(",
        "fn sqlite_replay_faults_are_atomic_and_concurrent_readers_see_complete_rows(",
        "fn performance_baseline_aggregates_local_metrics(",
        "fn optimization_policy_reads_metrics_and_measured_benchmark_evidence(",
    ] {
        assert!(
            !sqlite_tests.contains(moved_responsibility),
            "SQLite projection regression responsibility escaped into facade: {moved_responsibility}"
        );
    }
    assert!(
        sqlite.lines().count() < 500,
        "SQLite projection production module regrew beyond its session query extraction boundary"
    );
    assert!(sessions.lines().count() < 175);
    assert!(
        analytics.lines().count() < 450,
        "SQLite analytics module regrew beyond its ownership boundary"
    );
    assert!(latest_model_run.lines().count() < 100);
    assert!(latest_model_run_tests.lines().count() < 75);
    assert!(
        metrics.lines().count() < 375,
        "SQLite metric module regrew beyond its ownership boundary"
    );
    assert!(
        read_snapshot.lines().count() < 275,
        "SQLite read-only snapshot module regrew beyond its ownership boundary"
    );
    assert!(
        replay.lines().count() < 375,
        "SQLite replay module regrew beyond its ownership boundary"
    );
    assert!(
        schema.lines().count() < 400,
        "SQLite schema module regrew beyond its ownership boundary"
    );
    assert!(
        sqlite_tests.lines().count() < 150,
        "SQLite projection regression module regrew beyond its ownership boundary"
    );
    assert!(sqlite_projection_tests.lines().count() < 125);
    assert!(sqlite_recovery_tests.lines().count() < 125);
    assert!(sqlite_storage_tests.lines().count() < 500);
}
