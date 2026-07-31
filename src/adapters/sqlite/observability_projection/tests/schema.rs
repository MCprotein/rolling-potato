#[test]
fn migration_from_v6_keeps_legacy_benchmark_evidence_unqualified() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                 version INTEGER PRIMARY KEY,
                 description TEXT NOT NULL,
                 applied_at_ms INTEGER NOT NULL
             );
             INSERT INTO schema_migrations VALUES (6, 'v0_32_durable_conversation_resume', 1);
             CREATE TABLE benchmark_runs (
                 benchmark_run_id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL DEFAULT '',
                 model_run_id TEXT,
                 model_id TEXT NOT NULL,
                 benchmark_name TEXT NOT NULL,
                 fixture_id TEXT NOT NULL DEFAULT '',
                 fixture_sha256 TEXT NOT NULL DEFAULT '',
                 prompt_artifact_sha256 TEXT,
                 prompt_chars INTEGER,
                 claim_state TEXT NOT NULL DEFAULT 'not-comparable',
                 score REAL,
                 score_unit TEXT,
                 local_pass INTEGER,
                 expected_matches INTEGER,
                 expected_total INTEGER,
                 forbidden_matches INTEGER,
                 harness_ref TEXT NOT NULL,
                 dataset_ref TEXT,
                 backend_id TEXT,
                 latency_ms REAL,
                 tokens_per_second REAL,
                 prompt_tokens INTEGER,
                 completion_tokens INTEGER,
                 total_tokens INTEGER,
                 resource_pressure TEXT,
                 peak_rss_bytes INTEGER,
                 reproducibility_manifest TEXT NOT NULL DEFAULT '{}',
                 redacted_report TEXT NOT NULL DEFAULT '{}',
                 recorded_at_ms INTEGER NOT NULL
             );
             INSERT INTO benchmark_runs (
                 benchmark_run_id, model_id, benchmark_name, claim_state, score,
                 local_pass, harness_ref, recorded_at_ms
             ) VALUES (
                 'legacy-pass', 'qwen-test', 'legacy-smoke', 'measured-locally',
                 3.0, 1, 'legacy-harness', 1
             );",
        )
        .unwrap();

    migrate(&connection).unwrap();

    let migrated = connection
        .query_row(
            "SELECT evidence_schema_version, generation_status, finish_reason,
                    generation_profile_fingerprint
               FROM benchmark_runs
              WHERE benchmark_run_id = 'legacy-pass'",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .unwrap();
    let version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();

    assert_eq!(migrated, (None, None, None, None));
    assert_eq!(version, 7);
}
