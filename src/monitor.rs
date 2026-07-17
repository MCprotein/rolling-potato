//! Compatibility facade for surface-neutral monitoring use cases.

use std::path::PathBuf;

use crate::app::inference_adapter::model;
use crate::foundation::error::AppError;
use crate::runtime_core::observability::facade::{
    ModelMetricSummary, OptimizationPolicy, PerformanceBaseline, PrunePreview,
    ResourceSampleMetric, StoreStatus,
};
use crate::runtime_core::observability::monitor::{self as monitor_core, MonitorQueryPort};
use crate::surfaces::cli::command::MonitorExportFormat;
use crate::{adapters::filesystem::layout as paths, observability};

struct LocalMonitorQueryPort;

impl MonitorQueryPort for LocalMonitorQueryPort {
    fn status(&self) -> Result<StoreStatus, AppError> {
        observability::status()
    }

    fn latest_resource_sample(&self) -> Result<Option<ResourceSampleMetric>, AppError> {
        observability::latest_resource_sample()
    }

    fn runtime_ledger_path(&self) -> PathBuf {
        paths::runtime_ledger_file()
    }

    fn runtime_evidence_path(&self) -> PathBuf {
        paths::runtime_evidence_file()
    }

    fn model_summaries(&self) -> Result<Vec<ModelMetricSummary>, AppError> {
        observability::model_summaries()
    }

    fn model_candidate_summary(&self) -> String {
        model::candidate_summary()
    }

    fn performance_baseline(&self) -> Result<PerformanceBaseline, AppError> {
        observability::performance_baseline()
    }

    fn optimization_policy(&self) -> Result<OptimizationPolicy, AppError> {
        observability::optimization_policy()
    }

    fn export_jsonl(&self) -> Result<String, AppError> {
        observability::export_jsonl()
    }

    fn export_csv(&self) -> Result<String, AppError> {
        observability::export_csv()
    }

    fn prune_preview(&self, before_days: u64) -> Result<PrunePreview, AppError> {
        observability::prune_preview(before_days)
    }
}

pub fn status_report() -> Result<String, AppError> {
    monitor_core::status_report(&LocalMonitorQueryPort)
}

pub fn models_report() -> Result<String, AppError> {
    monitor_core::models_report(&LocalMonitorQueryPort)
}

pub fn baseline_report() -> Result<String, AppError> {
    monitor_core::baseline_report(&LocalMonitorQueryPort)
}

pub fn optimize_report() -> Result<String, AppError> {
    monitor_core::optimize_report(&LocalMonitorQueryPort)
}

pub fn export_report(format: MonitorExportFormat) -> Result<String, AppError> {
    let format = match format {
        MonitorExportFormat::Jsonl => monitor_core::MonitorExportFormat::Jsonl,
        MonitorExportFormat::Csv => monitor_core::MonitorExportFormat::Csv,
    };
    monitor_core::export_report(&LocalMonitorQueryPort, format)
}

pub fn prune_report(before_days: u64, dry_run: bool) -> Result<String, AppError> {
    monitor_core::prune_report(&LocalMonitorQueryPort, before_days, dry_run)
}
