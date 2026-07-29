use super::events;
use crate::app::collaboration_adapter::{subagent, subagent_result, team_state};
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team_execution::{detail_token, RuntimeIdentityBinding};
use crate::runtime_core::collaboration::team_reconciliation::{
    validate_action_ownership, validate_member_record,
};
use crate::runtime_core::collaboration::team_state::TeamMemberV1;
use std::collections::BTreeMap;

#[derive(Debug)]
pub(super) struct ReconciledMember {
    pub(super) lane: u32,
    pub(super) member_id: String,
    pub(super) record: subagent::SubagentRecordV1,
    pub(super) result: subagent_result::SubagentResultV1,
}

pub(super) fn collect(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
    manifest: &team_state::TeamManifestV1,
    ledger_events: &[ledger::ParsedLedgerEvent],
) -> Result<Vec<ReconciledMember>, AppError> {
    let admitted = admitted_bindings(identity, team, ledger_events)?;
    if admitted.len() != manifest.members.len() {
        return Err(AppError::blocked(format!(
            "team reconciliation result set 불완전\n- expected: {}\n- admitted: {}",
            manifest.members.len(),
            admitted.len()
        )));
    }

    manifest
        .members
        .iter()
        .map(|member| reconcile_member(identity, team, manifest, ledger_events, &admitted, member))
        .collect()
}

fn admitted_bindings(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
    events: &[ledger::ParsedLedgerEvent],
) -> Result<BTreeMap<u32, (String, String)>, AppError> {
    let mut admitted = BTreeMap::<u32, (String, String)>::new();
    for event in events.iter().filter(|event| {
        event.project_id == identity.project_id
            && event.session_id == identity.session_id
            && event.event_type == "team.worker.admitted"
            && detail_token(&event.details, "team_id") == Some(team.team_id.as_str())
    }) {
        let lane = detail_token(&event.details, "lane")
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| AppError::blocked("team admitted event lane binding 오류"))?;
        let member_id = detail_token(&event.details, "member_id")
            .ok_or_else(|| AppError::blocked("team admitted event member binding 누락"))?
            .to_string();
        let subagent_id = detail_token(&event.details, "subagent_id")
            .ok_or_else(|| AppError::blocked("team admitted event subagent binding 누락"))?
            .to_string();
        if let Some(existing) = admitted.get(&lane) {
            if existing != &(member_id.clone(), subagent_id.clone()) {
                return Err(AppError::blocked(
                    "team admitted event lane에 서로 다른 worker binding이 있습니다.",
                ));
            }
        } else {
            admitted.insert(lane, (member_id, subagent_id));
        }
    }
    Ok(admitted)
}

fn reconcile_member(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
    manifest: &team_state::TeamManifestV1,
    events: &[ledger::ParsedLedgerEvent],
    admitted: &BTreeMap<u32, (String, String)>,
    member: &TeamMemberV1,
) -> Result<ReconciledMember, AppError> {
    let (event_member_id, subagent_id) = admitted
        .get(&member.lane)
        .ok_or_else(|| AppError::blocked("team reconciliation lane binding 누락"))?;
    if event_member_id != &member.member_id {
        return Err(AppError::blocked(
            "team reconciliation manifest member binding 불일치",
        ));
    }
    let record = subagent::load_record(subagent_id)?;
    validate_member_record(
        &RuntimeIdentityBinding {
            project_id: &identity.project_id,
            session_id: &identity.session_id,
        },
        team,
        member,
        &record,
    )?;
    let result = subagent_result::load_completed_result(&record)?;
    let (action, target_path, source_hash) = validate_action_ownership(manifest, member, &result)?;
    let completed = events::has(
        events,
        identity,
        "team.worker.completed",
        &[
            ("team_id", team.team_id.as_str()),
            ("lane", &member.lane.to_string()),
            ("member_id", member.member_id.as_str()),
            ("subagent_id", record.subagent_id.as_str()),
            ("result_artifact_id", record.result_artifact_id.as_str()),
            ("evidence_id", record.evidence_id.as_str()),
        ],
    );
    let action_owned = events::has(
        events,
        identity,
        "team.worker.action-owned",
        &[
            ("team_id", team.team_id.as_str()),
            ("lane", &member.lane.to_string()),
            ("member_id", member.member_id.as_str()),
            ("subagent_id", record.subagent_id.as_str()),
            ("action", action),
            ("target_path", target_path),
            ("source_hash", source_hash),
        ],
    );
    if !completed || !action_owned {
        return Err(AppError::blocked(
            "team reconciliation worker completion/action receipt 누락",
        ));
    }
    Ok(ReconciledMember {
        lane: member.lane,
        member_id: member.member_id.clone(),
        record,
        result,
    })
}
