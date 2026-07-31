mod artifact_maintenance;
mod evaluation;
mod evidence;
mod promotion;
mod registry;
mod reports;
mod setup;

pub use artifact_maintenance::{cleanup_failed_report, verify_file_report};
pub(super) use evaluation::fetch_candidate_for_evaluation;
pub use evaluation::{eval_plan_report, fetch_candidate_for_evaluation_report};
pub use promotion::promote_candidate_report;
pub use registry::{
    default_artifact_path, default_report, install_candidate, registry_report, set_default_report,
};
pub use reports::{
    benchmark_plan_report, candidate_summary, download_plan_report, inspect_report, list_report,
    manifest_report,
};

use evidence::local_promotion_readiness;
pub(crate) use registry::{
    configured_model_id, prepare_bound_vision_projector, restore_default_selection,
    snapshot_default_selection, verified_vision_projector, DefaultSelectionSnapshot,
};
pub(crate) use setup::{
    activate_setup_model, configured_context_length, configured_runtime_spec,
    configured_vision_runtime, configured_vision_runtime_spec, prepare_setup_model, setup_options,
    ConfiguredRuntimeSpec, ConfiguredVisionRuntime,
};

#[cfg(test)]
use crate::adapters::filesystem::model_artifact::{model_artifact_path, promotion_evidence_path};
#[cfg(test)]
use crate::foundation::integrity as checksum;
#[cfg(test)]
use crate::runtime_core::inference::model::manifest::{
    find_candidate, source_backed_artifact, validate_install_ready,
};
#[cfg(test)]
use crate::runtime_core::inference::model::promotion::validate_promotion_evidence;
#[cfg(test)]
use evidence::{local_benchmark_status, promotion_benchmark_evidence};
#[cfg(test)]
use registry::registry_entry_json;

#[cfg(test)]
#[path = "model/promotion_tests.rs"]
mod promotion_tests;

#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;
