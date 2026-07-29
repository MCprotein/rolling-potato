use std::time::{SystemTime, UNIX_EPOCH};

use crate::foundation::error::AppError;
use crate::runtime_core::observability::html::{self, HtmlReportSnapshot, ReportData};

use super::{MonitorExportFormat, MonitorQueryPort};

pub(crate) fn export_report(
    port: &impl MonitorQueryPort,
    format: MonitorExportFormat,
) -> Result<String, AppError> {
    match format {
        MonitorExportFormat::Jsonl => port.export_jsonl(),
        MonitorExportFormat::Csv => port.export_csv(),
        MonitorExportFormat::Html => html_report(port),
    }
}

fn html_report(port: &impl MonitorQueryPort) -> Result<String, AppError> {
    let store = match port.status() {
        Ok(value) => ReportData::Available(value),
        Err(_) => ReportData::Unavailable,
    };
    let latest_resource = match port.latest_resource_sample() {
        Ok(value) => ReportData::Available(value),
        Err(_) => ReportData::Unavailable,
    };
    let model_summaries = match port.model_summaries() {
        Ok(value) => ReportData::Available(value),
        Err(_) => ReportData::Unavailable,
    };
    let optimization_policy = match port.optimization_policy() {
        Ok(value) => ReportData::Available(value),
        Err(_) => ReportData::Unavailable,
    };
    let generated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);

    Ok(html::render_report(&HtmlReportSnapshot {
        generated_at_ms,
        store,
        latest_resource,
        model_summaries,
        model_candidate_summary: port.model_candidate_summary(),
        optimization_policy,
    }))
}
