//! Self-contained, local-only HTML rendering for monitor snapshots.

use crate::runtime_core::observability::facade::{
    ModelMetricSummary, OptimizationPolicy, ResourceSampleMetric, StoreStatus,
};

#[path = "html/sections.rs"]
mod sections;
#[path = "html/template.rs"]
mod template;
#[cfg(test)]
#[path = "html/tests.rs"]
mod tests;
#[path = "html/text.rs"]
mod text;

pub(crate) enum ReportData<T> {
    Available(T),
    Unavailable,
}

pub(crate) struct HtmlReportSnapshot {
    pub generated_at_ms: u128,
    pub store: ReportData<StoreStatus>,
    pub latest_resource: ReportData<Option<ResourceSampleMetric>>,
    pub model_summaries: ReportData<Vec<ModelMetricSummary>>,
    pub model_candidate_summary: String,
    pub optimization_policy: ReportData<OptimizationPolicy>,
}

pub(crate) fn render_report(snapshot: &HtmlReportSnapshot) -> String {
    let mut html = String::with_capacity(12_000);
    template::render_document_start(&mut html, snapshot.generated_at_ms);
    sections::render_store_summary(&mut html, &snapshot.store);
    sections::render_resource(&mut html, &snapshot.latest_resource);
    sections::render_models(
        &mut html,
        &snapshot.model_summaries,
        &snapshot.model_candidate_summary,
    );
    sections::render_performance(&mut html, &snapshot.optimization_policy);
    sections::render_privacy(&mut html);
    template::render_document_end(&mut html);
    html
}
