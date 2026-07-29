//! Patch lifecycle, approval, verification, and recovery application adapter.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
#[cfg(test)]
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::extensions_adapter::{hooks, plugin, skill};
use crate::app::policy_adapter::{self as policy, Decision, PathMode};
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::app::workflow_adapter::transcript;
use crate::app::workflow_adapter::transition;
use crate::foundation::error::AppError;
use crate::runtime_core::patch::application::{
    self as application_domain, ApplyAdmission, ApplyResult, RollbackAdmission, RollbackResult,
};
use crate::runtime_core::patch::approval::{self as approval_domain, APPROVAL_TOKEN_BYTES};
use crate::runtime_core::patch::proposal::{
    self as proposal_domain, parse_header as parse_proposal_header, required_header,
    validate_proposal_id, PatchPreview, PreviewInput, ProposalRecord, RecordParse,
    MAX_PATCH_FILE_BYTES,
};
use crate::runtime_core::patch::verification::{
    self as verification_domain, RecoveryAdmission, VerificationPlan, VerificationResult,
};
use crate::surfaces::tui::outcome::unsupported_source_platform_outcome;
#[cfg(test)]
use crate::surfaces::tui::outcome::TuiEffect;
#[cfg(test)]
use crate::surfaces::tui::outcome::TuiOutcomeStatus;
use crate::surfaces::tui::outcome::{
    exact_tui_outcome, TuiOutcome, TuiOutcomeCode, TuiOutcomeContext,
};
use crate::surfaces::tui::runtime_bridge::{OneShotSecret, SelectionLease, TuiGateKind};

pub use crate::runtime_core::patch::proposal::{
    PatchProposalDetail, PatchProposalSummary, WorkflowProposal,
};

const MAX_PROPOSAL_RECORD_BYTES: usize = 2 * 1024 * 1024;

mod approval_dispatch;
mod approval_transaction;
mod execution;
mod guard;
mod proposal_api;
mod proposal_builder;
mod proposal_store;
mod resume;
mod shared;
mod terminal;
mod verification;
mod verification_evidence;
mod workflow_contract;
mod workflow_execution;

pub(crate) use approval_dispatch::approve_for_tui;
pub use approval_dispatch::approve_to_stdout;
use approval_dispatch::ApprovalDispatch;
#[cfg(test)]
use approval_dispatch::{
    approve_report, approve_report_for_intent, ensure_source_install_platform_supported,
};
use approval_transaction::{approve_prepared_skill_transaction, prepared_approval_receipt_exists};
pub(crate) use approval_transaction::{
    recover_prepared_approval_bundle, recover_prepared_verification_bundle,
};
use execution::{
    apply_proposal, build_verification_plan, format_verification_result, restore_from_rollback,
    run_verification,
};
use guard::{
    approval_prelock_test_barrier, load_workflow_under_approval_lock, restore_bytes, ApprovalLock,
};
pub(crate) use guard::{
    approval_projection_fault, approval_transaction_fault, verification_approval_transaction_fault,
};
pub use proposal_api::{prepare_workflow_proposal, preview_report};
use proposal_builder::{
    build_preview, current_source_hash, issue_approval_token, resolve_target_for, sha256_bytes,
    write_proposal_record,
};
pub(crate) use proposal_store::proposal_detail_for_workflow_bounded;
#[cfg(test)]
use proposal_store::summary_from_path;
use proposal_store::{
    dry_run_approval_report, load_proposal_record, rollback_path_for_record,
    validate_applied_proposal, validate_token_hash,
};
#[cfg(test)]
pub use resume::proposal_summaries;
pub(crate) use resume::resume_workflow_for_tui;
pub use resume::{preflight_resume_workflow, resume_workflow_report};
use shared::{display_none, read_decision_label, sha256_text};
pub use terminal::cancel_workflow_report;
#[cfg(test)]
pub(crate) use terminal::denial_phase_outcome_code;
#[cfg(test)]
pub use terminal::deny_pending_gate;
pub(crate) use terminal::{cancel_workflow_for_tui, deny_pending_gate_for_tui};
pub(crate) use verification::verify_for_tui;
pub use verification::verify_report;
pub use verification_evidence::{record_failing_test_before, validate_skill_verification};
pub(crate) use workflow_contract::is_stale_selection_error;
use workflow_contract::{
    failure_report, load_validated_approval_workflow, stale_selection_error, success_report,
    validate_outcome_id, validate_workflow_binding,
};
pub use workflow_execution::rotate_workflow_token_report;
use workflow_execution::{
    continue_approved_workflow, ensure_plugin_completion_event,
    ensure_plugin_completion_event_under_transition, finalize_verified_skill,
    plugin_completion_recovery_report, validate_completed_plugin_workflow,
    validate_completed_workflow, validate_failing_test_before, workflow_skill_runtime,
};

#[cfg(test)]
#[path = "patch_adapter/tests/mod.rs"]
mod tests;
