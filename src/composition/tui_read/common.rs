use crate::surfaces::tui::page::{tui_read_freshness as page_freshness, ProjectionStatus};
use crate::surfaces::tui::runtime_bridge::TuiFreshness;

use super::TuiReadPort;

pub(super) fn optional_metric(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "unavailable".to_string())
}

pub(super) fn freshness(
    port: &mut impl TuiReadPort,
    project_id: &str,
    canonical_events: u64,
    projected_events: Option<i64>,
) -> TuiFreshness {
    let projection_status: ProjectionStatus = port.projection_status(project_id);
    page_freshness(canonical_events, projected_events, projection_status)
}
