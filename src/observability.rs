use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::app::AppError;
use crate::ledger::{self, LedgerEvent, RuntimeIdentity};
use crate::paths;

const MIGRATION_VERSION: i64 = 1;
const MIGRATION_DESCRIPTION: &str = "phase2_initial_observability_schema";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreStatus {
    pub path: PathBuf,
    pub recovered_from: Option<PathBuf>,
    pub migration_version: i64,
    pub ledger_events: i64,
    pub sessions: i64,
    pub workflows: i64,
    pub model_runs: i64,
    pub token_records: i64,
    pub evidence_records: i64,
    pub stop_gate_results: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetricSummary {
    pub model_id: String,
    pub runs: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub avg_latency_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunePreview {
    pub cutoff_ms: u128,
    pub ledger_rows: i64,
    pub model_run_rows: i64,
    pub command_run_rows: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHistoryEntry {
    pub session_id: String,
    pub project_id: String,
    pub project_root: String,
    pub started_at_ms: i64,
    pub event_count: i64,
    pub last_event_at_ms: Option<i64>,
    pub last_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEventEntry {
    pub event_id: String,
    pub ts_ms: i64,
    pub event_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRunMetric {
    pub model_run_id: String,
    pub session_id: String,
    pub workflow_id: Option<String>,
    pub model_id: String,
    pub model_artifact_hash: Option<String>,
    pub backend_id: Option<String>,
    pub backend_version: Option<String>,
    pub quantization: Option<String>,
    pub context_limit_tokens: Option<u32>,
    pub started_at_ms: u128,
    pub first_token_latency_ms: Option<f64>,
    pub total_latency_ms: Option<f64>,
    pub prompt_eval_ms: Option<f64>,
    pub generation_eval_ms: Option<f64>,
    pub tokens_per_second: Option<f64>,
    pub cancelled: bool,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub context_tokens_used: u32,
    pub context_tokens_dropped: u32,
    pub ontology_tokens: u32,
    pub tool_summary_tokens: u32,
    pub max_output_tokens: Option<u32>,
}

pub fn initialize(identity: &RuntimeIdentity) -> Result<StoreStatus, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    record_session(&connection, identity)?;
    replay_ledger(&connection)?;
    project_sessions_from_events(&connection, identity)?;
    status_from_connection(&connection, recovered_from)
}

pub fn status() -> Result<StoreStatus, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    replay_ledger(&connection)?;
    status_from_connection(&connection, recovered_from)
}

pub fn project_event(event: &LedgerEvent) -> Result<(), AppError> {
    let (connection, _) = open_or_recover()?;
    insert_ledger_event(&connection, event)
}

pub fn model_summaries() -> Result<Vec<ModelMetricSummary>, AppError> {
    let (connection, _) = open_or_recover()?;
    let mut statement = connection
        .prepare(
            "SELECT token_usage.model_id,
                    COUNT(*) AS runs,
                    COALESCE(SUM(token_usage.prompt_tokens), 0),
                    COALESCE(SUM(token_usage.completion_tokens), 0),
                    COALESCE(SUM(token_usage.total_tokens), 0),
                    AVG(model_runs.total_latency_ms)
               FROM token_usage
          LEFT JOIN model_runs
                 ON token_usage.model_run_id = model_runs.model_run_id
              GROUP BY token_usage.model_id
              ORDER BY SUM(token_usage.total_tokens) DESC, token_usage.model_id ASC",
        )
        .map_err(sql_error("model metric query를 준비하지 못했습니다"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(ModelMetricSummary {
                model_id: row.get(0)?,
                runs: row.get(1)?,
                prompt_tokens: row.get(2)?,
                completion_tokens: row.get(3)?,
                total_tokens: row.get(4)?,
                avg_latency_ms: row.get(5)?,
            })
        })
        .map_err(sql_error("model metric query를 실행하지 못했습니다"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql_error("model metric 결과를 읽지 못했습니다"))
}

pub fn export_jsonl() -> Result<String, AppError> {
    let path = paths::runtime_ledger_file();
    if !path.exists() {
        return Ok(String::new());
    }

    fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "monitor JSONL export를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

pub fn export_csv() -> Result<String, AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger(&connection)?;

    let mut statement = connection
        .prepare(
            "SELECT event_id, ts_ms, event_type, project_id, session_id, summary
               FROM ledger_events
              ORDER BY ts_ms ASC, event_id ASC",
        )
        .map_err(sql_error("CSV export query를 준비하지 못했습니다"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(vec![
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.to_string(),
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ])
        })
        .map_err(sql_error("CSV export query를 실행하지 못했습니다"))?;

    let mut csv = String::from("event_id,ts_ms,event_type,project_id,session_id,summary\n");
    for row in rows {
        let row = row.map_err(sql_error("CSV export 결과를 읽지 못했습니다"))?;
        csv.push_str(
            &row.iter()
                .map(|value| csv_cell(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }

    Ok(csv)
}

pub fn prune_preview(before_days: u64) -> Result<PrunePreview, AppError> {
    let cutoff_ms = now_ms().saturating_sub((before_days as u128) * 24 * 60 * 60 * 1000);
    let cutoff = to_i64(cutoff_ms);
    let (connection, _) = open_or_recover()?;

    Ok(PrunePreview {
        cutoff_ms,
        ledger_rows: count_before(&connection, "ledger_events", "ts_ms", cutoff)?,
        model_run_rows: count_before(&connection, "model_runs", "started_at_ms", cutoff)?,
        command_run_rows: count_before(&connection, "command_runs", "started_at_ms", cutoff)?,
    })
}

pub fn session_history(limit: usize) -> Result<Vec<SessionHistoryEntry>, AppError> {
    let identity = ledger::current_identity();
    let (connection, _) = open_or_recover()?;
    replay_ledger(&connection)?;
    project_sessions_from_events(&connection, &identity)?;
    query_session_history(&connection, &identity.project_id, limit)
}

pub fn session_entry(session_id: &str) -> Result<Option<SessionHistoryEntry>, AppError> {
    let identity = ledger::current_identity();
    let (connection, _) = open_or_recover()?;
    replay_ledger(&connection)?;
    project_sessions_from_events(&connection, &identity)?;
    let entries = query_session_history(&connection, &identity.project_id, usize::MAX)?;
    Ok(entries
        .into_iter()
        .find(|entry| entry.session_id == session_id))
}

pub fn session_events(session_id: &str, limit: usize) -> Result<Vec<SessionEventEntry>, AppError> {
    let identity = ledger::current_identity();
    let (connection, _) = open_or_recover()?;
    replay_ledger(&connection)?;
    project_sessions_from_events(&connection, &identity)?;
    query_session_events(&connection, &identity.project_id, session_id, limit)
}

pub fn record_model_run(metric: &ModelRunMetric) -> Result<(), AppError> {
    let identity = ledger::current_identity();
    let (connection, _) = open_or_recover()?;
    record_session(&connection, &identity)?;
    replay_ledger(&connection)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO model_runs (
                model_run_id,
                session_id,
                workflow_id,
                model_id,
                model_artifact_hash,
                backend_id,
                backend_version,
                quantization,
                context_limit_tokens,
                started_at_ms,
                first_token_latency_ms,
                total_latency_ms,
                prompt_eval_ms,
                generation_eval_ms,
                tokens_per_second,
                cancelled
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                metric.model_run_id,
                metric.session_id,
                metric.workflow_id,
                metric.model_id,
                metric.model_artifact_hash,
                metric.backend_id,
                metric.backend_version,
                metric.quantization,
                metric.context_limit_tokens.map(i64::from),
                to_i64(metric.started_at_ms),
                metric.first_token_latency_ms,
                metric.total_latency_ms,
                metric.prompt_eval_ms,
                metric.generation_eval_ms,
                metric.tokens_per_second,
                if metric.cancelled { 1 } else { 0 },
            ],
        )
        .map_err(sql_error("model run metric을 저장하지 못했습니다"))?;

    connection
        .execute(
            "INSERT OR IGNORE INTO token_usage (
                token_usage_id,
                model_run_id,
                model_id,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                context_tokens_used,
                context_tokens_dropped,
                ontology_tokens,
                tool_summary_tokens,
                max_output_tokens
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                format!("token-{}", metric.model_run_id),
                metric.model_run_id,
                metric.model_id,
                i64::from(metric.prompt_tokens),
                i64::from(metric.completion_tokens),
                i64::from(metric.total_tokens),
                i64::from(metric.context_tokens_used),
                i64::from(metric.context_tokens_dropped),
                i64::from(metric.ontology_tokens),
                i64::from(metric.tool_summary_tokens),
                metric.max_output_tokens.map(i64::from),
            ],
        )
        .map_err(sql_error("token usage metric을 저장하지 못했습니다"))?;

    Ok(())
}

fn open_or_recover() -> Result<(Connection, Option<PathBuf>), AppError> {
    let path = paths::observability_db_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!(
                "observability 디렉터리를 만들지 못했습니다: {} ({err})",
                parent.display()
            ))
        })?;
    }

    match Connection::open(&path) {
        Ok(connection) => match migrate(&connection) {
            Ok(()) => Ok((connection, None)),
            Err(_err) if path.exists() => {
                drop(connection);
                let recovered = recover_corrupt_db(&path)?;
                let connection = Connection::open(&path)
                    .map_err(sql_error("복구 후 observability DB를 열지 못했습니다"))?;
                migrate(&connection)?;
                Ok((connection, Some(recovered)))
            }
            Err(err) => Err(err),
        },
        Err(_err) if path.exists() => {
            let recovered = recover_corrupt_db(&path)?;
            let connection = Connection::open(&path)
                .map_err(sql_error("복구 후 observability DB를 열지 못했습니다"))?;
            migrate(&connection)?;
            Ok((connection, Some(recovered)))
        }
        Err(err) => Err(AppError::runtime(format!(
            "observability DB를 열지 못했습니다: {} ({err})",
            path.display()
        ))),
    }
}

fn migrate(connection: &Connection) -> Result<(), AppError> {
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

            CREATE TABLE IF NOT EXISTS benchmark_runs (
                benchmark_run_id TEXT PRIMARY KEY,
                model_id TEXT NOT NULL,
                benchmark_name TEXT NOT NULL,
                score REAL,
                score_unit TEXT,
                harness_ref TEXT NOT NULL,
                dataset_ref TEXT,
                backend_id TEXT,
                recorded_at_ms INTEGER NOT NULL
            );
            ",
        )
        .map_err(sql_error(
            "observability schema migration을 적용하지 못했습니다",
        ))?;

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

fn record_session(connection: &Connection, identity: &RuntimeIdentity) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT OR IGNORE INTO sessions (
                session_id,
                project_id,
                project_root,
                started_at_ms,
                parent_session_id,
                branch_from_event_id,
                compacted_summary_path
             ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL)",
            params![
                identity.session_id,
                identity.project_id,
                identity.project_root,
                to_i64(now_ms())
            ],
        )
        .map_err(sql_error("session record를 저장하지 못했습니다"))?;
    Ok(())
}

fn replay_ledger(connection: &Connection) -> Result<(), AppError> {
    for event in ledger::read_runtime_events()? {
        connection
            .execute(
                "INSERT OR IGNORE INTO ledger_events (
                    event_id, ts_ms, event_type, project_id, session_id, summary
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event.event_id,
                    to_i64(event.ts_ms),
                    event.event_type,
                    event.project_id,
                    event.session_id,
                    event.summary
                ],
            )
            .map_err(sql_error("ledger replay projection을 저장하지 못했습니다"))?;
    }
    Ok(())
}

fn project_sessions_from_events(
    connection: &Connection,
    identity: &RuntimeIdentity,
) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT OR IGNORE INTO sessions (
                session_id,
                project_id,
                project_root,
                started_at_ms,
                parent_session_id,
                branch_from_event_id,
                compacted_summary_path
             )
             SELECT
                ledger_events.session_id,
                ledger_events.project_id,
                ?2,
                MIN(ledger_events.ts_ms),
                NULL,
                NULL,
                NULL
               FROM ledger_events
              WHERE ledger_events.project_id = ?1
           GROUP BY ledger_events.session_id,
                    ledger_events.project_id",
            params![identity.project_id, identity.project_root],
        )
        .map_err(sql_error("ledger session projection을 복원하지 못했습니다"))?;
    Ok(())
}

fn insert_ledger_event(connection: &Connection, event: &LedgerEvent) -> Result<(), AppError> {
    connection
        .execute(
            "INSERT OR IGNORE INTO ledger_events (
                event_id, ts_ms, event_type, project_id, session_id, summary
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.event_id,
                to_i64(event.ts_ms),
                event.event_type,
                event.project_id,
                event.session_id,
                event.summary
            ],
        )
        .map_err(sql_error("ledger event projection을 저장하지 못했습니다"))?;
    Ok(())
}

fn status_from_connection(
    connection: &Connection,
    recovered_from: Option<PathBuf>,
) -> Result<StoreStatus, AppError> {
    Ok(StoreStatus {
        path: paths::observability_db_file(),
        recovered_from,
        migration_version: count_scalar(
            connection,
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        )?,
        ledger_events: count_scalar(connection, "SELECT COUNT(*) FROM ledger_events")?,
        sessions: count_scalar(connection, "SELECT COUNT(*) FROM sessions")?,
        workflows: count_scalar(connection, "SELECT COUNT(*) FROM workflows")?,
        model_runs: count_scalar(connection, "SELECT COUNT(*) FROM model_runs")?,
        token_records: count_scalar(connection, "SELECT COUNT(*) FROM token_usage")?,
        evidence_records: count_scalar(connection, "SELECT COUNT(*) FROM evidence_records")?,
        stop_gate_results: count_scalar(connection, "SELECT COUNT(*) FROM stop_gate_results")?,
    })
}

fn count_scalar(connection: &Connection, sql: &str) -> Result<i64, AppError> {
    connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(sql_error("observability count query를 실행하지 못했습니다"))
}

fn count_before(
    connection: &Connection,
    table: &str,
    column: &str,
    cutoff_ms: i64,
) -> Result<i64, AppError> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE {column} < ?1");
    connection
        .query_row(&sql, params![cutoff_ms], |row| row.get(0))
        .map_err(sql_error(
            "monitor prune dry-run count를 실행하지 못했습니다",
        ))
}

fn query_session_history(
    connection: &Connection,
    project_id: &str,
    limit: usize,
) -> Result<Vec<SessionHistoryEntry>, AppError> {
    let sql = "
        SELECT
            sessions.session_id,
            sessions.project_id,
            sessions.project_root,
            sessions.started_at_ms,
            COUNT(ledger_events.event_id) AS event_count,
            MAX(ledger_events.ts_ms) AS last_event_at_ms,
            (
                SELECT latest.summary
                  FROM ledger_events latest
                 WHERE latest.session_id = sessions.session_id
                 ORDER BY latest.ts_ms DESC, latest.event_id DESC
                 LIMIT 1
            ) AS last_summary
          FROM sessions
     LEFT JOIN ledger_events
            ON ledger_events.session_id = sessions.session_id
         WHERE sessions.project_id = ?1
      GROUP BY sessions.session_id,
               sessions.project_id,
               sessions.project_root,
               sessions.started_at_ms
      ORDER BY COALESCE(MAX(ledger_events.ts_ms), sessions.started_at_ms) DESC,
               sessions.started_at_ms DESC
         LIMIT ?2";

    let mut statement = connection
        .prepare(sql)
        .map_err(sql_error("session history query를 준비하지 못했습니다"))?;
    let rows = statement
        .query_map(
            params![project_id, i64::try_from(limit).unwrap_or(i64::MAX)],
            |row| {
                Ok(SessionHistoryEntry {
                    session_id: row.get(0)?,
                    project_id: row.get(1)?,
                    project_root: row.get(2)?,
                    started_at_ms: row.get(3)?,
                    event_count: row.get(4)?,
                    last_event_at_ms: row.get(5)?,
                    last_summary: row.get(6)?,
                })
            },
        )
        .map_err(sql_error("session history query를 실행하지 못했습니다"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql_error("session history 결과를 읽지 못했습니다"))
}

fn query_session_events(
    connection: &Connection,
    project_id: &str,
    session_id: &str,
    limit: usize,
) -> Result<Vec<SessionEventEntry>, AppError> {
    let mut statement = connection
        .prepare(
            "
        SELECT event_id,
               ts_ms,
               event_type,
               summary
          FROM ledger_events
         WHERE project_id = ?1
           AND session_id = ?2
      ORDER BY ts_ms ASC,
               event_id ASC
         LIMIT ?3",
        )
        .map_err(sql_error("session event query를 준비하지 못했습니다"))?;
    let rows = statement
        .query_map(
            params![
                project_id,
                session_id,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| {
                Ok(SessionEventEntry {
                    event_id: row.get(0)?,
                    ts_ms: row.get(1)?,
                    event_type: row.get(2)?,
                    summary: row.get(3)?,
                })
            },
        )
        .map_err(sql_error("session event query를 실행하지 못했습니다"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(sql_error("session event 결과를 읽지 못했습니다"))
}

fn recover_corrupt_db(path: &std::path::Path) -> Result<PathBuf, AppError> {
    let recovered = path.with_extension(format!("sqlite.corrupt.{}", now_ms()));
    fs::rename(path, &recovered).map_err(|err| {
        AppError::runtime(format!(
            "손상된 observability DB를 보존 이동하지 못했습니다: {} -> {} ({err})",
            path.display(),
            recovered.display()
        ))
    })?;
    Ok(recovered)
}

fn csv_cell(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn sql_error(context: &'static str) -> impl FnOnce(rusqlite::Error) -> AppError {
    move |err| AppError::runtime(format!("{context}: {err}"))
}

fn to_i64(value: u128) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_cell_quotes_only_when_needed() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn record_model_run_updates_model_summary() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root =
            std::env::temp_dir().join(format!("rpotato-model-metric-test-{}", std::process::id()));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        record_model_run(&ModelRunMetric {
            model_run_id: "model-run-test".to_string(),
            session_id: "session-test".to_string(),
            workflow_id: None,
            model_id: "qwen-test".to_string(),
            model_artifact_hash: None,
            backend_id: Some("llama.cpp".to_string()),
            backend_version: None,
            quantization: None,
            context_limit_tokens: Some(4096),
            started_at_ms: 1,
            first_token_latency_ms: None,
            total_latency_ms: Some(100.0),
            prompt_eval_ms: None,
            generation_eval_ms: None,
            tokens_per_second: Some(120.0),
            cancelled: false,
            prompt_tokens: 10,
            completion_tokens: 12,
            total_tokens: 22,
            context_tokens_used: 10,
            context_tokens_dropped: 0,
            ontology_tokens: 0,
            tool_summary_tokens: 0,
            max_output_tokens: Some(64),
        })
        .unwrap();

        let summaries = model_summaries().unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].model_id, "qwen-test");
        assert_eq!(summaries[0].runs, 1);
        assert_eq!(summaries[0].prompt_tokens, 10);
        assert_eq!(summaries[0].completion_tokens, 12);
        assert_eq!(summaries[0].total_tokens, 22);
        assert_eq!(summaries[0].avg_latency_ms, Some(100.0));
    }
}
