use super::events::{stop_gate_error, stop_gate_failed};
use super::members::ReconciledMember;
use super::MAX_RECONCILIATION_BYTES;
use crate::adapters::filesystem::layout as paths;
use crate::app::collaboration_adapter::{subagent_result, team_state};
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team_reconciliation::parse_unique_evidence;
use std::collections::BTreeSet;

pub(super) fn verify_member_inputs(
    team: &team_state::TeamStateV1,
    members: &[ReconciledMember],
    expected_reconciliation: &str,
) -> Result<(), AppError> {
    let installed = state::read_regular_file_bounded(
        &paths::project_team_reconciliation_file(&team.team_id),
        MAX_RECONCILIATION_BYTES,
        "team reconciliation artifact",
    )?;
    if installed != expected_reconciliation {
        return stop_gate_failed(team, "reconciliation artifact binding mismatch");
    }
    if members
        .iter()
        .any(|member| !member.result.validation_gaps.is_empty())
    {
        return stop_gate_failed(team, "unresolved worker validation gaps");
    }
    for member in members {
        subagent_result::verify_completed_source_freshness(&member.record)
            .map_err(|_| stop_gate_error(team, "missing or stale worker evidence"))?;
    }
    Ok(())
}

pub(super) fn merge_parent(
    team: &team_state::TeamStateV1,
    team_evidence: &[String],
) -> Result<state::WorkflowRecord, AppError> {
    if state::active_workflow_id()?.as_deref() != Some(team.parent_workflow_id.as_str()) {
        return Err(AppError::blocked(
            "team parent evidence merge active workflow binding 불일치",
        ));
    }
    let original = state::load_workflow_revision(&team.parent_workflow_id, team.parent_revision)?;
    if original.artifact_hash != team.parent_artifact_hash
        || original.project_id != team.project_id
        || original.session_id != team.session_id
        || original.is_terminal()
    {
        return Err(AppError::blocked(
            "team parent evidence merge planned parent binding 불일치",
        ));
    }
    let mut evidence = workflow_evidence(&original)?;
    let mut unique = evidence.iter().cloned().collect::<BTreeSet<_>>();
    for evidence_id in team_evidence {
        if !unique.insert(evidence_id.clone()) {
            return Err(AppError::blocked(
                "team parent evidence merge duplicate evidence binding",
            ));
        }
        evidence.push(evidence_id.clone());
    }
    let expected_evidence = evidence.join(",");
    let current = state::load_workflow(&team.parent_workflow_id)?;
    if current == original {
        let mut merged = original.clone();
        merged.skill_evidence = expected_evidence;
        return state::checkpoint_workflow(merged, original.revision);
    }
    if is_expected_merged_parent(&original, &current, &expected_evidence) {
        return Ok(current);
    }
    Err(AppError::blocked(
        "team parent evidence merge exact binding 불일치",
    ))
}

pub(super) fn verify_stop_gate(
    team: &team_state::TeamStateV1,
    parent: &state::WorkflowRecord,
    evidence_ids: &[String],
    expected_reconciliation: &str,
) -> Result<(), AppError> {
    let original = state::load_workflow_revision(&team.parent_workflow_id, team.parent_revision)?;
    let mut expected_evidence = workflow_evidence(&original)?;
    expected_evidence.extend(evidence_ids.iter().cloned());
    if !is_expected_merged_parent(&original, parent, &expected_evidence.join(",")) {
        return stop_gate_failed(team, "parent evidence checkpoint mismatch");
    }
    let installed = state::read_regular_file_bounded(
        &paths::project_team_reconciliation_file(&team.team_id),
        MAX_RECONCILIATION_BYTES,
        "team reconciliation artifact",
    )?;
    if installed != expected_reconciliation {
        return stop_gate_failed(team, "reconciliation artifact stale");
    }
    Ok(())
}

fn is_expected_merged_parent(
    original: &state::WorkflowRecord,
    current: &state::WorkflowRecord,
    expected_evidence: &str,
) -> bool {
    let mut expected = original.clone();
    expected.revision = original.revision.saturating_add(1);
    expected.previous_hash = original.artifact_hash.clone();
    expected.artifact_hash = current.artifact_hash.clone();
    expected.skill_evidence = expected_evidence.to_string();
    current == &expected
}

fn workflow_evidence(parent: &state::WorkflowRecord) -> Result<Vec<String>, AppError> {
    parse_unique_evidence(&parent.skill_evidence)
}
