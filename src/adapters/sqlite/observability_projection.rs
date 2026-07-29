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
mod lifecycle;
mod metrics;
mod port;
mod queries;
mod read_snapshot;
mod replay;
mod schema;
mod sessions;
mod store;

use analytics::{
    latest_model_run_for_session_from_connection, model_summaries, model_summaries_from_connection,
    optimization_policy, performance_baseline,
};
pub(crate) use lifecycle::{converge_from_events, project_event_with_ordinal};
pub use lifecycle::{initialize, status};
use metrics::{
    benchmark_run_reports, latest_resource_sample, record_benchmark_run, record_model_run,
    record_resource_sample,
};
pub(crate) use port::SqliteObservabilityProjection;
#[cfg(test)]
use queries::csv_cell;
pub use queries::{
    export_csv, export_jsonl, latest_model_run_for_session_read_only, monitor_snapshot_read_only,
    prune_preview, status_read_only,
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
use store::{
    count_before, count_scalar, i64_to_u128, i64_to_u32, now_ms, open_or_recover,
    option_i64_to_bool, option_i64_to_u32, option_i64_to_u64, sql_error, status_from_connection,
    to_i64,
};

#[cfg(test)]
#[path = "observability_projection/tests.rs"]
mod tests;
