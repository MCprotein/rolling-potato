use super::*;

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

    fn latest_model_run_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<LatestModelRunSnapshot>, AppError> {
        latest_model_run_for_session_read_only(session_id)
    }
}
