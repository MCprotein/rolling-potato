//! Compatibility facade for observability domain records and SQLite projection operations.

pub use crate::adapters::sqlite::observability_projection::{
    benchmark_run_reports, export_csv, export_jsonl, initialize, latest_resource_sample,
    model_summaries, monitor_snapshot_read_only, optimization_policy, performance_baseline,
    project_event, prune_preview, record_benchmark_run, record_model_run, record_resource_sample,
    session_entry, session_events, session_history, status, status_read_only,
};
pub(crate) use crate::adapters::sqlite::observability_projection::{
    converge_from_events, project_event_with_ordinal,
};
pub use crate::runtime_core::observability::facade::{
    BenchmarkRunMetric, BenchmarkRunReport, ModelRunMetric, ResourceSampleMetric,
    SessionHistoryEntry, StoreStatus,
};
