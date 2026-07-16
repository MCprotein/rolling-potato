use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team_execution::{detail_token, RuntimeIdentityBinding};
use crate::runtime_core::collaboration::team_reconciliation::{
    parse_unique_evidence, render_reconciliation as render_reconciliation_policy,
    validate_action_ownership, validate_reconciliation_binding, validate_reconciliation_stage,
    ReconciliationMemberBinding,
};
use crate::{
    adapters::filesystem::layout as paths, adapters::filesystem::lease, ledger, observability,
    state, subagent, subagent_result, team_state,
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_RECONCILIATION_BYTES: u64 = 65_536;

#[derive(Debug)]
struct ReconciledMember {
    lane: u32,
    member_id: String,
    record: subagent::SubagentRecordV1,
    result: subagent_result::SubagentResultV1,
}

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
    let members = collect_members(&identity, &team, &manifest, &events)?;
    let reconciliation_body = render_reconciliation(&team, &members);
    let reconciliation_hash = state::sha256_text(&reconciliation_body);
    install_reconciliation(team_id, &reconciliation_body)?;
    append_event_once(
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

    verify_stop_inputs(&team, &members, &reconciliation_body)?;
    if team.stage == team_state::TeamStage::Review {
        team = team_state::advance_state(team_id, team_state::TeamStage::Verify, None, None)?;
    }

    let evidence_ids = members
        .iter()
        .map(|member| member.record.evidence_id.clone())
        .collect::<Vec<_>>();
    let merged_parent = merge_parent_evidence(&team, &evidence_ids)?;
    append_event_once(
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
        &[("team_id", team.team_id.as_str()), ("reconciliation_hash", reconciliation_hash.as_str())],
    )?;
    if team.stage == team_state::TeamStage::Verify {
        team = team_state::advance_state(team_id, team_state::TeamStage::Merge, None, None)?;
    }

    verify_team_stop_gate(&team, &merged_parent, &evidence_ids, &reconciliation_body)?;
    append_event_once(
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

    append_event_once(
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

fn collect_members(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
    manifest: &team_state::TeamManifestV1,
    events: &[ledger::ParsedLedgerEvent],
) -> Result<Vec<ReconciledMember>, AppError> {
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
    if admitted.len() != manifest.members.len() {
        return Err(AppError::blocked(format!(
            "team reconciliation result set 불완전\n- expected: {}\n- admitted: {}",
            manifest.members.len(),
            admitted.len()
        )));
    }

    let mut reconciled = Vec::with_capacity(manifest.members.len());
    for member in &manifest.members {
        let (event_member_id, subagent_id) = admitted
            .get(&member.lane)
            .ok_or_else(|| AppError::blocked("team reconciliation lane binding 누락"))?;
        if event_member_id != &member.member_id {
            return Err(AppError::blocked(
                "team reconciliation manifest member binding 불일치",
            ));
        }
        let record = subagent::load_record(subagent_id)?;
        validate_member_record(identity, team, member, &record)?;
        let result = subagent_result::load_completed_result(&record)?;
        let (action, target_path, source_hash) =
            validate_action_ownership(manifest, member, &result)?;
        if !has_event(
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
        ) || !has_event(
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
        ) {
            return Err(AppError::blocked(
                "team reconciliation worker completion/action receipt 누락",
            ));
        }
        reconciled.push(ReconciledMember {
            lane: member.lane,
            member_id: member.member_id.clone(),
            record,
            result,
        });
    }
    Ok(reconciled)
}

fn validate_member_record(
    identity: &ledger::RuntimeIdentity,
    team: &team_state::TeamStateV1,
    member: &team_state::TeamMemberV1,
    record: &subagent::SubagentRecordV1,
) -> Result<(), AppError> {
    if record.project_id != identity.project_id
        || record.session_id != identity.session_id
        || record.parent_workflow_id != team.parent_workflow_id
        || record.parent_revision != team.parent_revision
        || record.parent_artifact_hash != team.parent_artifact_hash
        || record.status != subagent::SubagentStatus::Completed
        || record.role.as_str() != member.role
        || record.task_hash != member.task_hash
        || record.declared_tools != member.tools
        || record.read_paths != member.read_paths
        || record.write_paths != member.write_paths
        || record.timeout_ms != member.timeout_ms
        || record.requested_max_tokens != member.max_tokens
    {
        return Err(AppError::blocked(
            "team reconciliation worker immutable binding 불일치",
        ));
    }
    Ok(())
}

fn verify_stop_inputs(
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

fn merge_parent_evidence(
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

fn verify_team_stop_gate(
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

fn render_reconciliation(team: &team_state::TeamStateV1, members: &[ReconciledMember]) -> String {
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
    render_reconciliation_policy(team, &bindings)
}

fn install_reconciliation(team_id: &str, body: &str) -> Result<(), AppError> {
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
    state::atomic_replace_bytes(&path, body.as_bytes())
}

fn append_event_once(
    identity: &ledger::RuntimeIdentity,
    event_type: &str,
    summary: &str,
    details: &str,
    match_fields: &[(&str, &str)],
) -> Result<(), AppError> {
    if has_event(
        &ledger::read_runtime_events()?,
        identity,
        event_type,
        match_fields,
    ) {
        return Ok(());
    }
    let event = ledger::new_event_for(identity, event_type, summary, details);
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

fn has_event(
    events: &[ledger::ParsedLedgerEvent],
    identity: &ledger::RuntimeIdentity,
    event_type: &str,
    fields: &[(&str, &str)],
) -> bool {
    events.iter().any(|event| {
        event.project_id == identity.project_id
            && event.session_id == identity.session_id
            && event.event_type == event_type
            && fields
                .iter()
                .all(|(key, value)| detail_token(&event.details, key) == Some(*value))
    })
}

fn stop_gate_failed<T>(team: &team_state::TeamStateV1, reason: &str) -> Result<T, AppError> {
    Err(stop_gate_error(team, reason))
}

fn stop_gate_error(team: &team_state::TeamStateV1, reason: &str) -> AppError {
    let persistence = ledger::validated_current_identity()
        .and_then(|identity| {
            append_event_once(
                &identity,
                "team.stop-gate.failed",
                "team evidence-required stop gate failed",
                &format!(
                    "team_id={} reason={}",
                    team.team_id,
                    reason.replace(' ', "-")
                ),
                &[
                    ("team_id", team.team_id.as_str()),
                    ("reason", &reason.replace(' ', "-")),
                ],
            )
        })
        .err()
        .map(|error| format!("\n- stop-gate failure event 저장 실패: {}", error.message))
        .unwrap_or_default();
    AppError::blocked(format!(
        "team stop gate 차단\n- team id: {}\n- 이유: {}\n- 동작: parent evidence merge 또는 completion을 진행하지 않습니다.{}",
        team.team_id, reason, persistence
    ))
}
