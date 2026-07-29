use std::path::PathBuf;

use crate::foundation::error::AppError;
use crate::runtime_core::observability::facade::{
    ModelMetricSummary, OptimizationPolicy, PerformanceBaseline, PrunePreview,
    ResourceSampleMetric, StoreStatus,
};

#[path = "monitor/format.rs"]
mod format;
#[path = "monitor/metric.rs"]
mod metric;
#[path = "monitor/policy.rs"]
mod policy;
#[path = "monitor/report.rs"]
mod report;

pub(crate) use metric::{baseline_report, models_report, status_report};
pub(crate) use policy::{optimize_report, prune_report};
pub(crate) use report::export_report;

pub(crate) enum MonitorExportFormat {
    Jsonl,
    Csv,
    Html,
}

pub(crate) trait MonitorQueryPort {
    fn status(&self) -> Result<StoreStatus, AppError>;

    fn latest_resource_sample(&self) -> Result<Option<ResourceSampleMetric>, AppError>;

    fn runtime_ledger_path(&self) -> PathBuf;

    fn runtime_evidence_path(&self) -> PathBuf;

    fn model_summaries(&self) -> Result<Vec<ModelMetricSummary>, AppError>;

    fn model_candidate_summary(&self) -> String;

    fn performance_baseline(&self) -> Result<PerformanceBaseline, AppError>;

    fn optimization_policy(&self) -> Result<OptimizationPolicy, AppError>;

    fn export_jsonl(&self) -> Result<String, AppError>;

    fn export_csv(&self) -> Result<String, AppError>;

    fn prune_preview(&self, before_days: u64) -> Result<PrunePreview, AppError>;
}

#[cfg(test)]
#[path = "monitor/tests.rs"]
mod tests;
