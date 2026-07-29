//! Team reconciliation coordinator.

use crate::adapters::filesystem::layout as paths;
use crate::adapters::filesystem::lease;
use crate::app::collaboration_adapter::team_state;
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team_execution::RuntimeIdentityBinding;
use crate::runtime_core::collaboration::team_reconciliation::{
    validate_reconciliation_binding, validate_reconciliation_stage,
};

mod artifact;
mod events;
mod evidence;
mod members;

const MAX_RECONCILIATION_BYTES: u64 = 65_536;

pub fn reconcile_report(team_id: &str) -> Result<String, AppError> {
    let _operation = lease::RecoverableLease::acquire(
        paths::project_team_operation_lock(team_id),
        "team operation",
    )?;
    let identity = ledger::validated_current_identity()?;
    let mut team = team_state::load_state(team_id)?;
    let manifest = team_state::load_manifest(team_id)?;
    validate_reconciliation_binding(
        &RuntimeIdentityBinding {
            project_id: &identity.project_id,
            session_id: &identity.session_id,
        },
        &team,
        &manifest,
    )?;
    validate_reconciliation_stage(&team)?;
    if team_state::cancellation_requested(team_id)? {
        return Err(AppError::blocked(format!(
            "team reconcile cancellation 차단\n- team id: {team_id}"
        )));
    }

    let events = ledger::read_runtime_events()?;
    let members = members::collect(&identity, &team, &manifest, &events)?;
    let reconciliation_body = artifact::render(&team, &members);
    let reconciliation_hash = state::sha256_text(&reconciliation_body);
    artifact::install(team_id, &reconciliation_body)?;
    events::append_once(
        &identity,
        "team.result-set.reconciled",
        "team result set reconciled",
        &format!(
            "team_id={} reconciliation_hash={} member_count={} manifest_hash={}",
            team.team_id,
            reconciliation_hash,
            members.len(),
            team.manifest_hash
        ),
        &[
            ("team_id", team.team_id.as_str()),
            ("reconciliation_hash", reconciliation_hash.as_str()),
        ],
    )?;
    if team.stage == team_state::TeamStage::Execute {
        team = team_state::advance_state(team_id, team_state::TeamStage::Review, None, None)?;
    }

    evidence::verify_member_inputs(&team, &members, &reconciliation_body)?;
    if team.stage == team_state::TeamStage::Review {
        team = team_state::advance_state(team_id, team_state::TeamStage::Verify, None, None)?;
    }

    let evidence_ids = members
        .iter()
        .map(|member| member.record.evidence_id.clone())
        .collect::<Vec<_>>();
    let merged_parent = evidence::merge_parent(&team, &evidence_ids)?;
    events::append_once(
        &identity,
        "team.evidence.merged",
        "team evidence merged",
        &format!(
            "team_id={} parent_workflow_id={} parent_revision={} evidence_count={} reconciliation_hash={}",
            team.team_id,
            merged_parent.workflow_id,
            merged_parent.revision,
            evidence_ids.len(),
            reconciliation_hash
        ),
        &[
            ("team_id", team.team_id.as_str()),
            ("reconciliation_hash", reconciliation_hash.as_str()),
        ],
    )?;
    if team.stage == team_state::TeamStage::Verify {
        team = team_state::advance_state(team_id, team_state::TeamStage::Merge, None, None)?;
    }

    evidence::verify_stop_gate(&team, &merged_parent, &evidence_ids, &reconciliation_body)?;
    events::append_once(
        &identity,
        "team.stop-gate.passed",
        "team evidence-required stop gate passed",
        &format!(
            "team_id={} parent_workflow_id={} evidence_count={} reconciliation_hash={}",
            team.team_id,
            merged_parent.workflow_id,
            evidence_ids.len(),
            reconciliation_hash
        ),
        &[
            ("team_id", team.team_id.as_str()),
            ("reconciliation_hash", reconciliation_hash.as_str()),
        ],
    )?;
    if team.stage == team_state::TeamStage::Merge {
        team = team_state::advance_state(team_id, team_state::TeamStage::Report, None, None)?;
    }

    events::append_once(
        &identity,
        "team.report.completed",
        "team completion report recorded",
        &format!(
            "team_id={} member_count={} evidence_count={} reconciliation_hash={}",
            team.team_id,
            members.len(),
            evidence_ids.len(),
            reconciliation_hash
        ),
        &[
            ("team_id", team.team_id.as_str()),
            ("reconciliation_hash", reconciliation_hash.as_str()),
        ],
    )?;
    if team.stage == team_state::TeamStage::Report {
        team = team_state::advance_state(team_id, team_state::TeamStage::Complete, None, None)?;
    }
    if team.stage != team_state::TeamStage::Complete {
        return Err(AppError::blocked(format!(
            "team reconcile completion stage 불일치: {}",
            team.stage.as_str()
        )));
    }

    Ok(format!(
        "team reconcile\n- status: completed\n- team id: {}\n- stage: {}\n- members: {}\n- evidence merged: {}\n- parent workflow: {}\n- parent revision: {}\n- reconciliation artifact: {}\n- reconciliation hash: {}\n- stop gate: passed",
        team.team_id,
        team.stage.as_str(),
        members.len(),
        evidence_ids.len(),
        merged_parent.workflow_id,
        merged_parent.revision,
        paths::project_team_reconciliation_file(team_id).display(),
        reconciliation_hash,
    ))
}
