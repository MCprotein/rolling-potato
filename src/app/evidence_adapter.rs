//! Verification evidence persistence and stop-gate application adapter.

mod artifact_pointer;
mod recording;
mod stop_gate;
mod store;

#[allow(unused_imports)]
pub use crate::runtime_core::knowledge::evidence::{
    stale_policy_summary, EvidenceValidation, VerificationEvidence,
};
#[allow(unused_imports)]
pub use artifact_pointer::{validate_artifact_pointer, validate_report};
pub use recording::record_patch_verification;
pub use stop_gate::{evaluate_patch_stop_gate, validate_patch_stop_gate};
pub use store::store_status;
pub(crate) use store::store_status_bounded;

#[cfg(test)]
#[path = "evidence_adapter/tests.rs"]
mod tests;
