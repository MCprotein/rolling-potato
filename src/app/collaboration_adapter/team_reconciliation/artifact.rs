use super::members::ReconciledMember;
use super::MAX_RECONCILIATION_BYTES;
use crate::adapters::filesystem::layout as paths;
use crate::app::collaboration_adapter::team_state;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team_reconciliation::{
    render_reconciliation, ReconciliationMemberBinding,
};

pub(super) fn render(team: &team_state::TeamStateV1, members: &[ReconciledMember]) -> String {
    let bindings = members
        .iter()
        .map(|member| ReconciliationMemberBinding {
            lane: member.lane,
            member_id: &member.member_id,
            subagent_id: &member.record.subagent_id,
            result_artifact_id: &member.record.result_artifact_id,
            result_artifact_hash: &member.record.result_artifact_hash,
            evidence_id: &member.record.evidence_id,
            evidence_hash: &member.record.evidence_hash,
        })
        .collect::<Vec<_>>();
    render_reconciliation(team, &bindings)
}

pub(super) fn install(team_id: &str, body: &str) -> Result<(), AppError> {
    if body.is_empty() || body.len() as u64 > MAX_RECONCILIATION_BYTES {
        return Err(AppError::blocked(
            "team reconciliation artifact 크기 상한 위반",
        ));
    }
    let path = paths::project_team_reconciliation_file(team_id);
    if path.exists() {
        let existing = state::read_regular_file_bounded(
            &path,
            MAX_RECONCILIATION_BYTES,
            "team reconciliation artifact",
        )?;
        if existing != body {
            return Err(AppError::blocked(
                "team reconciliation deterministic artifact 충돌",
            ));
        }
        return Ok(());
    }
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, body.as_bytes())
}
