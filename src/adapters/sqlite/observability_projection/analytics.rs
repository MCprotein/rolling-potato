//! Read-only observability analytics facade.

use super::*;

mod latest_model_run;
mod model_summaries;
mod optimization_policy;
mod performance_baseline;
mod statistics;

pub(super) use latest_model_run::latest_model_run_for_session_from_connection;
pub(super) use model_summaries::{model_summaries, model_summaries_from_connection};
pub(super) use optimization_policy::optimization_policy;
pub(super) use performance_baseline::performance_baseline;
