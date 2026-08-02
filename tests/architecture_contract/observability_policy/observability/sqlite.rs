include!("sqlite/analytics.rs");

pub(super) fn assert_sqlite_observability_projection() {
    let sqlite = fs::read_to_string("src/adapters/sqlite/observability_projection.rs").unwrap();
    let metrics_path = "src/adapters/sqlite/observability_projection/metrics.rs";
    let lifecycle_path = "src/adapters/sqlite/observability_projection/lifecycle.rs";
    let port_path = "src/adapters/sqlite/observability_projection/port.rs";
    let queries_path = "src/adapters/sqlite/observability_projection/queries.rs";
    let read_snapshot_path = "src/adapters/sqlite/observability_projection/read_snapshot.rs";
    let replay_path = "src/adapters/sqlite/observability_projection/replay.rs";
    let schema_path = "src/adapters/sqlite/observability_projection/schema.rs";
    let sessions_path = "src/adapters/sqlite/observability_projection/sessions.rs";
    let store_path = "src/adapters/sqlite/observability_projection/store.rs";
    let sqlite_tests_path = "src/adapters/sqlite/observability_projection/tests.rs";
    let sqlite_projection_tests_path =
        "src/adapters/sqlite/observability_projection/tests/projection.rs";
    let sqlite_recovery_tests_path =
        "src/adapters/sqlite/observability_projection/tests/recovery.rs";
    let sqlite_schema_tests_path = "src/adapters/sqlite/observability_projection/tests/schema.rs";
    let sqlite_storage_tests_path = "src/adapters/sqlite/observability_projection/tests/storage.rs";
    for path in [
        metrics_path,
        lifecycle_path,
        port_path,
        queries_path,
        read_snapshot_path,
        replay_path,
        schema_path,
        sessions_path,
        store_path,
        sqlite_tests_path,
        sqlite_projection_tests_path,
        sqlite_recovery_tests_path,
        sqlite_schema_tests_path,
        sqlite_storage_tests_path,
    ] {
        assert!(Path::new(path).is_file(), "missing SQLite owner: {path}");
    }
    let metrics = fs::read_to_string(metrics_path).unwrap();
    let lifecycle = fs::read_to_string(lifecycle_path).unwrap();
    let port = fs::read_to_string(port_path).unwrap();
    let queries = fs::read_to_string(queries_path).unwrap();
    let read_snapshot = fs::read_to_string(read_snapshot_path).unwrap();
    let replay = fs::read_to_string(replay_path).unwrap();
    let schema = fs::read_to_string(schema_path).unwrap();
    let sessions = fs::read_to_string(sessions_path).unwrap();
    let store = fs::read_to_string(store_path).unwrap();
    let sqlite_tests = fs::read_to_string(sqlite_tests_path).unwrap();
    let sqlite_projection_tests = fs::read_to_string(sqlite_projection_tests_path).unwrap();
    let sqlite_recovery_tests = fs::read_to_string(sqlite_recovery_tests_path).unwrap();
    let sqlite_schema_tests = fs::read_to_string(sqlite_schema_tests_path).unwrap();
    let sqlite_storage_tests = fs::read_to_string(sqlite_storage_tests_path).unwrap();
    let projection_port_impl = "impl ObservabilityProjectionPort for SqliteObservabilityProjection";
    assert!(
        port.contains(projection_port_impl),
        "SQLite adapter is missing: {projection_port_impl}"
    );
    assert!(
        !sqlite.contains(projection_port_impl),
        "SQLite projection port implementation escaped into the facade"
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
    assert_sqlite_observability_analytics();
    assert!(
        sqlite.lines().any(|line| line == "mod metrics;"),
        "SQLite projection does not register the metric owner"
    );
    for owner in ["lifecycle", "port", "queries", "store"] {
        let module = format!("mod {owner};");
        assert!(
            sqlite.lines().any(|line| line == module),
            "SQLite projection does not register its {owner} owner"
        );
    }
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
        "pub fn initialize(",
        "pub fn status(",
        "pub(crate) fn project_event_with_ordinal(",
        "pub(crate) fn converge_from_events(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "lifecycle responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            lifecycle.contains(responsibility),
            "SQLite lifecycle owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub fn status_read_only(",
        "pub fn monitor_snapshot_read_only(",
        "pub fn latest_model_run_for_session_read_only(",
        "pub fn export_jsonl(",
        "pub fn export_csv(",
        "pub fn prune_preview(",
        "pub(super) fn csv_cell(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "query responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            queries.contains(responsibility),
            "SQLite query owner is missing: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) fn open_or_recover(",
        "pub(super) fn status_from_connection(",
        "pub(super) fn count_scalar(",
        "pub(super) fn count_before(",
        "fn recover_corrupt_db(",
        "pub(super) fn sql_error(",
    ] {
        assert!(
            !sqlite.contains(responsibility),
            "store responsibility escaped into projection facade: {responsibility}"
        );
        assert!(
            store.contains(responsibility),
            "SQLite store owner is missing: {responsibility}"
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
    assert!(sqlite.lines().count() < 100);
    assert!(lifecycle.lines().count() < 75);
    assert!(port.lines().count() < 175);
    assert!(queries.lines().count() < 150);
    assert!(store.lines().count() < 175);
    assert!(sessions.lines().count() < 175);
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
    assert!(sqlite_schema_tests.lines().count() < 125);
    assert!(sqlite_storage_tests.lines().count() < 500);
}
