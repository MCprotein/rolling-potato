use crate::app::collaboration_adapter::team_state;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team::{
    admission_event_type, admission_summary, continuation_decision, decision_label,
    dispatch_event_type, dispatch_status, dispatch_summary, evaluate_ownership_gate,
    evaluate_policy_gate, governor_event_type, governor_status, governor_summary,
    is_team_runtime_event, overall_status, policy_write_paths, pressure_from_status,
    OwnershipCheck, PolicyCheck,
};
use crate::runtime_core::inference::resource;

mod admission;
mod admission_report;
mod dispatch;
mod governor;
mod report_format;
mod status;

pub use admission_report::admission_report;
pub use dispatch::dispatch_report;
pub use governor::governor_report;
pub use status::status_report;

#[cfg(test)]
#[path = "team/tests.rs"]
mod tests;
