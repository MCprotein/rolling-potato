//! Compatibility facade for surface-neutral observability projection ports.

use crate::adapters::sqlite::observability_projection::SqliteObservabilityProjection;
use crate::foundation::error::AppError;
use crate::ledger;
use crate::runtime_core::observability::facade::ObservabilityProjectionPort;
use crate::runtime_core::workflow::storage_compat::ledger::{
    LedgerEvent, ParsedLedgerEvent, RuntimeIdentity,
};

pub use crate::runtime_core::observability::facade::{
    BenchmarkRunMetric, BenchmarkRunReport, ModelRunMetric, ResourceSampleMetric,
    SessionHistoryEntry, StoreStatus,
};

const PROJECTION: SqliteObservabilityProjection = SqliteObservabilityProjection;

pub fn initialize(identity: &RuntimeIdentity) -> Result<StoreStatus, AppError> {
    PROJECTION.initialize(identity, &ledger::read_runtime_events()?)
}

pub fn status() -> Result<StoreStatus, AppError> {
    PROJECTION.status(&ledger::read_runtime_events()?)
}

pub fn status_read_only() -> Result<StoreStatus, AppError> {
    PROJECTION.status_read_only()
}

pub fn monitor_snapshot_read_only(
    limit: usize,
) -> Result<crate::runtime_core::observability::facade::MonitorProjectionSnapshot, AppError> {
    PROJECTION.monitor_snapshot_read_only(limit)
}

pub fn project_event(event: &LedgerEvent) -> Result<(), AppError> {
    PROJECTION.project_event(event, &ledger::read_runtime_events()?)
}

pub(crate) fn project_event_with_ordinal(
    event: &LedgerEvent,
    ordinal: u64,
) -> Result<(), AppError> {
    PROJECTION.project_event_with_ordinal(event, ordinal)
}

pub(crate) fn converge_from_events(events: &[ParsedLedgerEvent]) -> Result<(), AppError> {
    PROJECTION.converge_from_events(events)
}

pub fn model_summaries(
) -> Result<Vec<crate::runtime_core::observability::facade::ModelMetricSummary>, AppError> {
    PROJECTION.model_summaries()
}

pub fn performance_baseline(
) -> Result<crate::runtime_core::observability::facade::PerformanceBaseline, AppError> {
    PROJECTION.performance_baseline(&ledger::read_runtime_events()?)
}

pub fn optimization_policy(
) -> Result<crate::runtime_core::observability::facade::OptimizationPolicy, AppError> {
    PROJECTION.optimization_policy(&ledger::read_runtime_events()?)
}

pub fn export_jsonl() -> Result<String, AppError> {
    PROJECTION.export_jsonl()
}

pub fn export_csv() -> Result<String, AppError> {
    PROJECTION.export_csv(&ledger::read_runtime_events()?)
}

pub fn prune_preview(
    before_days: u64,
) -> Result<crate::runtime_core::observability::facade::PrunePreview, AppError> {
    PROJECTION.prune_preview(before_days)
}

pub fn session_history(limit: usize) -> Result<Vec<SessionHistoryEntry>, AppError> {
    let identity = ledger::validated_current_identity()?;
    PROJECTION.session_history(&identity, &ledger::read_runtime_events()?, limit)
}

pub fn session_entry(session_id: &str) -> Result<Option<SessionHistoryEntry>, AppError> {
    let identity = ledger::validated_current_identity()?;
    PROJECTION.session_entry(&identity, &ledger::read_runtime_events()?, session_id)
}

pub fn session_events(
    session_id: &str,
    limit: usize,
) -> Result<Vec<crate::runtime_core::observability::facade::SessionEventEntry>, AppError> {
    let identity = ledger::validated_current_identity()?;
    PROJECTION.session_events(
        &identity,
        &ledger::read_runtime_events()?,
        session_id,
        limit,
    )
}

pub fn record_model_run(metric: &ModelRunMetric) -> Result<(), AppError> {
    let identity = ledger::validated_current_identity()?;
    PROJECTION.record_model_run(&identity, &ledger::read_runtime_events()?, metric)
}

pub fn record_resource_sample(metric: &ResourceSampleMetric) -> Result<(), AppError> {
    let identity = ledger::validated_current_identity()?;
    PROJECTION.record_resource_sample(&identity, &ledger::read_runtime_events()?, metric)
}

pub fn record_benchmark_run(metric: &BenchmarkRunMetric) -> Result<(), AppError> {
    let identity = ledger::validated_current_identity()?;
    PROJECTION.record_benchmark_run(&identity, &ledger::read_runtime_events()?, metric)
}

pub fn benchmark_run_reports() -> Result<Vec<BenchmarkRunReport>, AppError> {
    PROJECTION.benchmark_run_reports(&ledger::read_runtime_events()?)
}

pub fn latest_resource_sample() -> Result<Option<ResourceSampleMetric>, AppError> {
    PROJECTION.latest_resource_sample()
}
