use rusqlite::{params, Connection};

use super::{now_ms, sql_error, to_i64};
use crate::foundation::error::AppError;

const MIGRATION_VERSION: i64 = 6;
const MIGRATION_DESCRIPTION: &str = "v0_32_durable_conversation_resume";

pub(super) fn migrate(connection: &Connection) -> Result<(), AppError> {
    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                applied_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ledger_events (
                event_id TEXT PRIMARY KEY,
                ts_ms INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                project_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                summary TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS resource_samples (
                resource_sample_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                backend_id TEXT NOT NULL,
                pid INTEGER NOT NULL,
                process_cpu_percent REAL,
                average_rss_bytes INTEGER,
                peak_rss_bytes INTEGER,
                disk_bytes INTEGER,
                sample_count INTEGER NOT NULL DEFAULT 1,
                pressure_status TEXT NOT NULL,
                recorded_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                session_id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                project_root TEXT NOT NULL,
                started_at_ms INTEGER NOT NULL,
                parent_session_id TEXT,
                branch_from_event_id TEXT,
                compacted_summary_path TEXT
            );

            CREATE TABLE IF NOT EXISTS workflows (
                workflow_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                state TEXT NOT NULL,
                active_skill_id TEXT,
                updated_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS workflow_transitions (
                from_state TEXT NOT NULL,
                to_state TEXT NOT NULL,
                reason TEXT NOT NULL,
                PRIMARY KEY (from_state, to_state)
            );

            CREATE TABLE IF NOT EXISTS checkpoint_records (
                checkpoint_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                workflow_id TEXT,
                checkpoint_type TEXT NOT NULL,
                artifact_pointer TEXT NOT NULL,
                artifact_hash TEXT,
                recorded_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS model_runs (
                model_run_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                workflow_id TEXT,
                model_id TEXT NOT NULL,
                model_artifact_hash TEXT,
                backend_id TEXT,
                backend_version TEXT,
                quantization TEXT,
                context_limit_tokens INTEGER,
                started_at_ms INTEGER NOT NULL,
                first_token_latency_ms REAL,
                total_latency_ms REAL,
                prompt_eval_ms REAL,
                generation_eval_ms REAL,
                tokens_per_second REAL,
                cancelled INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS token_usage (
                token_usage_id TEXT PRIMARY KEY,
                model_run_id TEXT,
                model_id TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                context_tokens_used INTEGER NOT NULL DEFAULT 0,
                context_tokens_dropped INTEGER NOT NULL DEFAULT 0,
                ontology_tokens INTEGER NOT NULL DEFAULT 0,
                tool_summary_tokens INTEGER NOT NULL DEFAULT 0,
                max_output_tokens INTEGER
            );

            CREATE TABLE IF NOT EXISTS backend_runs (
                backend_run_id TEXT PRIMARY KEY,
                backend_id TEXT NOT NULL,
                backend_version TEXT,
                startup_ms REAL,
                health_latency_ms REAL,
                peak_rss_bytes INTEGER,
                disk_bytes INTEGER,
                crash_count INTEGER NOT NULL DEFAULT 0,
                active_session_count INTEGER NOT NULL DEFAULT 0,
                recorded_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tool_calls (
                tool_call_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                workflow_id TEXT,
                tool_name TEXT NOT NULL,
                decision TEXT NOT NULL,
                success INTEGER NOT NULL,
                exit_code_class TEXT,
                latency_ms REAL,
                recorded_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS command_runs (
                command_run_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                command_class TEXT NOT NULL,
                exit_code INTEGER,
                exit_code_class TEXT,
                redacted_summary TEXT NOT NULL,
                artifact_pointer TEXT,
                started_at_ms INTEGER NOT NULL,
                finished_at_ms INTEGER
            );

            CREATE TABLE IF NOT EXISTS guard_results (
                guard_result_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                guard_name TEXT NOT NULL,
                passed INTEGER NOT NULL,
                rejection_reason TEXT,
                recorded_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS stop_gate_results (
                stop_gate_result_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                workflow_id TEXT,
                passed INTEGER NOT NULL,
                missing_evidence_count INTEGER NOT NULL DEFAULT 0,
                recorded_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS evidence_records (
                evidence_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                workflow_id TEXT,
                evidence_type TEXT NOT NULL,
                artifact_pointer TEXT NOT NULL,
                artifact_hash TEXT,
                stale_after_ms INTEGER,
                recorded_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS transcript_records (
                record_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                workflow_id TEXT NOT NULL,
                ledger_event_id TEXT NOT NULL,
                event_ordinal INTEGER NOT NULL,
                record_kind TEXT NOT NULL,
                causal_id TEXT NOT NULL,
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                source_pointers_json TEXT NOT NULL,
                artifact_pointer TEXT NOT NULL,
                artifact_hash TEXT NOT NULL,
                recorded_at_ms INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS benchmark_runs (
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
            ",
        )
        .map_err(sql_error(
            "observability schema migration을 적용하지 못했습니다",
        ))?;

    ensure_column(
        connection,
        "benchmark_runs",
        "session_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "benchmark_runs",
        "fixture_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "benchmark_runs",
        "fixture_sha256",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(connection, "benchmark_runs", "model_run_id", "TEXT")?;
    ensure_column(
        connection,
        "benchmark_runs",
        "prompt_artifact_sha256",
        "TEXT",
    )?;
    ensure_column(connection, "benchmark_runs", "prompt_chars", "INTEGER")?;
    ensure_column(
        connection,
        "benchmark_runs",
        "claim_state",
        "TEXT NOT NULL DEFAULT 'not-comparable'",
    )?;
    ensure_column(connection, "benchmark_runs", "local_pass", "INTEGER")?;
    ensure_column(connection, "benchmark_runs", "expected_matches", "INTEGER")?;
    ensure_column(connection, "benchmark_runs", "expected_total", "INTEGER")?;
    ensure_column(connection, "benchmark_runs", "forbidden_matches", "INTEGER")?;
    ensure_column(connection, "benchmark_runs", "latency_ms", "REAL")?;
    ensure_column(connection, "benchmark_runs", "tokens_per_second", "REAL")?;
    ensure_column(connection, "benchmark_runs", "prompt_tokens", "INTEGER")?;
    ensure_column(connection, "benchmark_runs", "completion_tokens", "INTEGER")?;
    ensure_column(connection, "benchmark_runs", "total_tokens", "INTEGER")?;
    ensure_column(connection, "benchmark_runs", "resource_pressure", "TEXT")?;
    ensure_column(connection, "benchmark_runs", "peak_rss_bytes", "INTEGER")?;
    ensure_column(
        connection,
        "benchmark_runs",
        "reproducibility_manifest",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_column(
        connection,
        "transcript_records",
        "ledger_event_id",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "transcript_records",
        "event_ordinal",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    connection
        .execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_transcript_ledger_event
                 ON transcript_records (ledger_event_id);
             CREATE INDEX IF NOT EXISTS idx_transcript_session_order
                 ON transcript_records (session_id, event_ordinal);",
        )
        .map_err(sql_error("transcript 순서 index 생성 실패"))?;
    ensure_column(
        connection,
        "benchmark_runs",
        "redacted_report",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;

    connection
        .execute(
            "INSERT OR IGNORE INTO schema_migrations (version, description, applied_at_ms)
             VALUES (?1, ?2, ?3)",
            params![MIGRATION_VERSION, MIGRATION_DESCRIPTION, to_i64(now_ms())],
        )
        .map_err(sql_error("schema migration 기록을 저장하지 못했습니다"))?;

    for (from_state, to_state, reason) in [
        ("idle", "running", "workflow started"),
        ("running", "complete", "stop gate passed"),
        ("running", "failed", "unrecoverable failure"),
        ("running", "cancelled", "user or runtime cancellation"),
        (
            "failed",
            "running",
            "explicit resume from recoverable failure",
        ),
        ("cancelled", "running", "explicit resume after cancellation"),
    ] {
        connection
            .execute(
                "INSERT OR IGNORE INTO workflow_transitions (from_state, to_state, reason)
                 VALUES (?1, ?2, ?3)",
                params![from_state, to_state, reason],
            )
            .map_err(sql_error("workflow transition table을 저장하지 못했습니다"))?;
    }

    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), AppError> {
    let existing_columns = {
        let pragma = format!("PRAGMA table_info({table})");
        let mut statement = connection
            .prepare(&pragma)
            .map_err(sql_error("schema column query를 준비하지 못했습니다"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(sql_error("schema column query를 실행하지 못했습니다"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(sql_error("schema column 결과를 읽지 못했습니다"))?
    };

    if existing_columns.iter().any(|existing| existing == column) {
        return Ok(());
    }

    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
    connection
        .execute(&sql, [])
        .map_err(sql_error("schema column 추가를 적용하지 못했습니다"))?;
    Ok(())
}
