use crate::app::collaboration_adapter::team_state;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team_execution::detail_token;

pub(super) fn append_once(
    identity: &ledger::RuntimeIdentity,
    event_type: &str,
    summary: &str,
    details: &str,
    match_fields: &[(&str, &str)],
) -> Result<(), AppError> {
    if has(
        &ledger::read_runtime_events()?,
        identity,
        event_type,
        match_fields,
    ) {
        return Ok(());
    }
    let event = ledger::new_event_for(identity, event_type, summary, details);
    let appended = ledger::append_event(&event)?;
    observability::project_event_with_ordinal(&event, appended.ordinal)
}

pub(super) fn has(
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

pub(super) fn stop_gate_failed<T>(
    team: &team_state::TeamStateV1,
    reason: &str,
) -> Result<T, AppError> {
    Err(stop_gate_error(team, reason))
}

pub(super) fn stop_gate_error(team: &team_state::TeamStateV1, reason: &str) -> AppError {
    let persistence = ledger::validated_current_identity()
        .and_then(|identity| {
            append_once(
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
