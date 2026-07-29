#[test]
fn v0377_observability_ports_own_projection_and_monitoring_boundaries() {
    for target in [
        "src/adapters/sqlite/ledger_projection.rs",
        "src/adapters/sqlite/observability_projection.rs",
        "src/adapters/sqlite/transcript_projection.rs",
        "src/runtime_core/observability/facade.rs",
        "src/runtime_core/observability/html.rs",
        "src/runtime_core/observability/monitor.rs",
        "src/runtime_core/workflow/application/projection_barrier.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.7 observability owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    assert!(
        runtime_core
            .lines()
            .any(|line| line == "pub(crate) mod observability;"),
        "runtime observability owner is not crate-private"
    );
    let observability_mod = fs::read_to_string("src/runtime_core/observability/mod.rs").unwrap();
    for owner in ["facade", "html", "monitor"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            observability_mod.lines().any(|line| line == expected),
            "runtime observability child is not crate-private: {owner}"
        );
    }

    let facade = fs::read_to_string("src/runtime_core/observability/facade.rs").unwrap();
    assert!(
        facade.contains("trait ObservabilityProjectionPort"),
        "observability facade does not own the projection port"
    );
    assert!(
        facade.contains("trait CanonicalLedgerReadPort"),
        "observability facade does not own the canonical ledger read port"
    );
    assert!(
        facade.contains("trait CanonicalTranscriptReadPort")
            && facade.contains("trait CanonicalProjectionReadPort"),
        "observability facade does not own the canonical transcript projection port"
    );
    for record in [
        "struct StoreStatus",
        "struct MonitorProjectionSnapshot",
        "struct ModelRunMetric",
        "struct SessionHistoryEntry",
    ] {
        assert!(
            facade.contains(record),
            "observability facade is missing projection record: {record}"
        );
    }

    let monitor = fs::read_to_string("src/runtime_core/observability/monitor.rs").unwrap();
    for rule in [
        "trait MonitorQueryPort",
        "status_report",
        "models_report",
        "baseline_report",
        "optimize_report",
        "prune_report",
        "export_report",
    ] {
        assert!(
            monitor.contains(rule),
            "monitor owner is missing use case: {rule}"
        );
    }
    for (owner, line_budget, rules) in [
        (
            "format",
            75,
            &["fn display_optional_u64", "fn score_label"][..],
        ),
        (
            "metric",
            250,
            &["fn status_report", "fn models_report", "fn baseline_report"][..],
        ),
        (
            "policy",
            175,
            &["fn optimize_report", "fn prune_report"][..],
        ),
        ("report", 100, &["fn export_report", "fn html_report"][..]),
        (
            "tests",
            250,
            &[
                "status_report_is_rendered_from_port_data",
                "html_export_preserves_all_sections_when_queries_are_unavailable",
            ][..],
        ),
    ] {
        let relative = format!("monitor/{owner}.rs");
        assert!(
            monitor.contains(&relative),
            "monitor facade does not register {owner}"
        );
        let source =
            fs::read_to_string(format!("src/runtime_core/observability/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "monitor owner {owner} exceeded its {line_budget}-line budget"
        );
        for rule in rules {
            assert!(
                source.contains(rule),
                "monitor owner {owner} is missing contract: {rule}"
            );
        }
    }
    assert!(monitor.lines().count() < 100);
    let html = fs::read_to_string("src/runtime_core/observability/html.rs").unwrap();
    for rule in ["struct HtmlReportSnapshot", "fn render_report"] {
        assert!(
            html.contains(rule),
            "HTML monitor owner is missing contract: {rule}"
        );
    }
    assert!(html.lines().count() < 100);
    for (owner, line_budget, rules) in [
        (
            "template",
            175,
            &["Content-Security-Policy", "fn render_document_start"][..],
        ),
        (
            "sections",
            250,
            &["fn render_store_summary", "fn render_performance"][..],
        ),
        ("text", 125, &["fn safe_html_text", "fn escape_html"][..]),
        (
            "tests",
            200,
            &["report_is_self_contained", "empty_and_unavailable"][..],
        ),
    ] {
        let relative = format!("html/{owner}.rs");
        assert!(
            html.contains(&relative),
            "HTML monitor facade does not register {owner}"
        );
        let source =
            fs::read_to_string(format!("src/runtime_core/observability/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "HTML monitor owner {owner} exceeded its {line_budget}-line budget"
        );
        for rule in rules {
            assert!(
                source.contains(rule),
                "HTML monitor owner {owner} is missing contract: {rule}"
            );
        }
    }

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

    let transcript = fs::read_to_string("src/adapters/sqlite/transcript_projection.rs").unwrap();
    assert!(
        transcript.contains("INSERT OR REPLACE INTO transcript_records"),
        "transcript SQLite adapter does not own row installation"
    );
    assert!(
        transcript.contains("CanonicalTranscriptReadPort") && !transcript.contains("crate::app"),
        "transcript SQLite adapter does not use the inverted canonical read port"
    );
    let ledger = fs::read_to_string("src/adapters/sqlite/ledger_projection.rs").unwrap();
    assert!(
        ledger.contains("fn validate_event_sequence"),
        "ledger SQLite adapter does not own sequence validation"
    );

    let barrier =
        fs::read_to_string("src/runtime_core/workflow/application/projection_barrier.rs").unwrap();
    for rule in [
        "trait ProjectionBarrierRecoveryPort",
        "fn recover_through_projection_barrier",
    ] {
        assert!(
            barrier.contains(rule),
            "projection barrier owner is missing policy: {rule}"
        );
    }
    let recovery = fs::read_to_string("src/runtime_core/workflow/application/recovery.rs").unwrap();
    assert!(
        !recovery.contains("fn recover_through_projection_barrier"),
        "workflow recovery still owns the moved projection barrier"
    );

    for (facade_path, forbidden) in [
        ("src/app/observability_adapter.rs", "rusqlite"),
        ("src/app/monitor_adapter.rs", "performance baseline\\n"),
        ("src/app/workflow_adapter/ledger.rs", "rusqlite::Connection"),
    ] {
        let source = fs::read_to_string(facade_path).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(forbidden),
            "legacy facade retains moved implementation: {facade_path} -> {forbidden}"
        );
    }
    assert!(!Path::new("src/monitor.rs").exists());
    let monitor_adapter = fs::read_to_string("src/app/monitor_adapter.rs").unwrap();
    assert!(monitor_adapter.contains("impl MonitorQueryPort for LocalMonitorQueryPort"));
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod monitor;"));
    assert!(!Path::new("src/observability.rs").exists());
    let observability_adapter = fs::read_to_string("src/app/observability_adapter.rs").unwrap();
    assert!(observability_adapter.contains("impl CanonicalLedgerReadPort"));
    assert!(observability_adapter.contains("impl CanonicalTranscriptReadPort"));
    assert!(!main.lines().any(|line| line == "mod observability;"));
}
