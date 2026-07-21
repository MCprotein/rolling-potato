use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::inference::resource;
use crate::runtime_core::observability::facade::{
    BenchmarkEvidenceSummary, BenchmarkRunMetric, BenchmarkRunReport, CanonicalProjectionReadPort,
    LatestModelRunSnapshot, ModelMetricSummary, ModelRunMetric, MonitorProjectionSnapshot,
    ObservabilityProjectionPort, OptimizationPolicy, PerformanceBaseline, PerformanceGroupSummary,
    PressureStateSummary, PrunePreview, ResourceSampleMetric, SessionEventEntry,
    SessionHistoryEntry, StoreStatus,
};
#[cfg(test)]
use crate::runtime_core::observability::facade::{
    CanonicalLedgerReadPort, CanonicalTranscriptReadPort,
};
use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerEvent, ParsedLedgerEvent, RuntimeIdentity,
};

mod analytics;
mod metrics;
mod read_snapshot;
mod replay;
mod schema;
mod sessions;
use analytics::{
    latest_model_run_from_connection, model_summaries, model_summaries_from_connection,
    optimization_policy, performance_baseline,
};
use metrics::{
    benchmark_run_reports, latest_resource_sample, record_benchmark_run, record_model_run,
    record_resource_sample,
};
use read_snapshot::open_read_only;
#[cfg(test)]
use read_snapshot::open_read_only_path;
#[cfg(test)]
use replay::project_workflow_checkpoint;
use replay::{
    insert_ledger_event, project_sessions_from_events, record_session, replay_ledger_events,
};
use schema::migrate;
pub use sessions::{session_entry, session_events, session_history};

pub(crate) struct SqliteObservabilityProjection;

impl ObservabilityProjectionPort for SqliteObservabilityProjection {
    fn initialize(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<StoreStatus, AppError> {
        initialize(identity, ledger)
    }

    fn status(&self, ledger: &dyn CanonicalProjectionReadPort) -> Result<StoreStatus, AppError> {
        status(ledger)
    }

    fn status_read_only(&self) -> Result<StoreStatus, AppError> {
        status_read_only()
    }

    fn monitor_snapshot_read_only(
        &self,
        limit: usize,
    ) -> Result<MonitorProjectionSnapshot, AppError> {
        monitor_snapshot_read_only(limit)
    }

    fn project_event_with_ordinal(
        &self,
        event: &LedgerEvent,
        ordinal: u64,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<(), AppError> {
        project_event_with_ordinal(event, ordinal, ledger)
    }

    fn converge_from_events(
        &self,
        events: &[ParsedLedgerEvent],
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<(), AppError> {
        converge_from_events(events, ledger)
    }

    fn model_summaries(&self) -> Result<Vec<ModelMetricSummary>, AppError> {
        model_summaries()
    }

    fn performance_baseline(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<PerformanceBaseline, AppError> {
        performance_baseline(ledger)
    }

    fn optimization_policy(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<OptimizationPolicy, AppError> {
        optimization_policy(ledger)
    }

    fn export_jsonl(&self) -> Result<String, AppError> {
        export_jsonl()
    }

    fn export_csv(&self, ledger: &dyn CanonicalProjectionReadPort) -> Result<String, AppError> {
        export_csv(ledger)
    }

    fn prune_preview(&self, before_days: u64) -> Result<PrunePreview, AppError> {
        prune_preview(before_days)
    }

    fn session_history(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        limit: usize,
    ) -> Result<Vec<SessionHistoryEntry>, AppError> {
        session_history(identity, ledger, limit)
    }

    fn session_entry(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        session_id: &str,
    ) -> Result<Option<SessionHistoryEntry>, AppError> {
        session_entry(identity, ledger, session_id)
    }

    fn session_events(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SessionEventEntry>, AppError> {
        session_events(identity, ledger, session_id, limit)
    }

    fn record_model_run(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &ModelRunMetric,
    ) -> Result<(), AppError> {
        record_model_run(identity, ledger, metric)
    }

    fn record_resource_sample(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &ResourceSampleMetric,
    ) -> Result<(), AppError> {
        record_resource_sample(identity, ledger, metric)
    }

    fn record_benchmark_run(
        &self,
        identity: &RuntimeIdentity,
        ledger: &dyn CanonicalProjectionReadPort,
        metric: &BenchmarkRunMetric,
    ) -> Result<(), AppError> {
        record_benchmark_run(identity, ledger, metric)
    }

    fn benchmark_run_reports(
        &self,
        ledger: &dyn CanonicalProjectionReadPort,
    ) -> Result<Vec<BenchmarkRunReport>, AppError> {
        benchmark_run_reports(ledger)
    }

    fn latest_resource_sample(&self) -> Result<Option<ResourceSampleMetric>, AppError> {
        latest_resource_sample()
    }

    fn latest_model_run(&self) -> Result<Option<LatestModelRunSnapshot>, AppError> {
        latest_model_run_read_only()
    }
}

pub fn initialize(
    identity: &RuntimeIdentity,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<StoreStatus, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    record_session(&connection, identity)?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    project_sessions_from_events(&connection, identity)?;
    status_from_connection(&connection, recovered_from)
}

pub fn status(ledger: &dyn CanonicalProjectionReadPort) -> Result<StoreStatus, AppError> {
    let (connection, recovered_from) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;
    status_from_connection(&connection, recovered_from)
}

pub fn status_read_only() -> Result<StoreStatus, AppError> {
    let connection = open_read_only()?;
    status_from_connection(&connection, None)
}

pub fn monitor_snapshot_read_only(limit: usize) -> Result<MonitorProjectionSnapshot, AppError> {
    let connection = open_read_only()?;
    Ok(MonitorProjectionSnapshot {
        status: status_from_connection(&connection, None)?,
        model_summaries: model_summaries_from_connection(&connection, limit)?,
    })
}

pub fn latest_model_run_read_only() -> Result<Option<LatestModelRunSnapshot>, AppError> {
    let connection = open_read_only()?;
    latest_model_run_from_connection(&connection)
}

pub(crate) fn project_event_with_ordinal(
    event: &LedgerEvent,
    ordinal: u64,
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    let ordinal = i64::try_from(ordinal)
        .map_err(|_| AppError::blocked("observability event ordinal 범위 초과"))?;
    let (connection, _) = open_or_recover()?;
    insert_ledger_event(&connection, event, ordinal, ledger)
}

pub(crate) fn converge_from_events(
    events: &[ParsedLedgerEvent],
    ledger: &dyn CanonicalProjectionReadPort,
) -> Result<(), AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, events, ledger)
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

pub fn export_csv(ledger: &dyn CanonicalProjectionReadPort) -> Result<String, AppError> {
    let (connection, _) = open_or_recover()?;
    replay_ledger_events(&connection, &ledger.read_events()?, ledger)?;

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
        resource_sample_rows: count_before(
            &connection,
            "resource_samples",
            "recorded_at_ms",
            cutoff,
        )?,
    })
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
        transcript_records: count_scalar(connection, "SELECT COUNT(*) FROM transcript_records")?,
        model_runs: count_scalar(connection, "SELECT COUNT(*) FROM model_runs")?,
        token_records: count_scalar(connection, "SELECT COUNT(*) FROM token_usage")?,
        resource_samples: count_scalar(connection, "SELECT COUNT(*) FROM resource_samples")?,
        benchmark_runs: count_scalar(connection, "SELECT COUNT(*) FROM benchmark_runs")?,
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

fn i64_to_u128(value: i64) -> u128 {
    u128::try_from(value).unwrap_or_default()
}

fn i64_to_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

fn option_i64_to_u32(value: Option<i64>) -> Option<u32> {
    value.and_then(|value| u32::try_from(value).ok())
}

fn option_i64_to_bool(value: Option<i64>) -> Option<bool> {
    value.map(|value| value != 0)
}

fn option_i64_to_u64(value: Option<i64>) -> Option<u64> {
    value.and_then(|value| u64::try_from(value).ok())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "observability_projection/tests.rs"]
mod tests;
