use super::*;

pub(super) fn latest_team_runtime_event(
    identity: &ledger::RuntimeIdentity,
) -> Result<Option<ledger::ParsedLedgerEvent>, AppError> {
    let mut events = ledger::read_runtime_events()?;
    events.retain(|event| {
        event.project_id == identity.project_id && is_team_runtime_event(&event.event_type)
    });
    events.sort_by(|left, right| {
        left.ts_ms
            .cmp(&right.ts_ms)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    Ok(events.pop())
}

pub(super) fn format_policy_checks(checks: &[PolicyCheck]) -> String {
    if checks.is_empty() {
        return "  - 없음".to_string();
    }

    checks
        .iter()
        .map(|check| {
            format!(
                "  - {}: {} -> {} ({}, approval: {}, reason: {})",
                check.target_type,
                check.target,
                decision_label(check.decision),
                check.class,
                check.approval_prompt,
                check.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn format_ownership_checks(checks: &[OwnershipCheck]) -> String {
    if checks.is_empty() {
        return "  - 없음".to_string();
    }

    checks
        .iter()
        .map(|check| {
            format!(
                "  - lane {}: {} -> {} (normalized: {}, reason: {})",
                check.lane, check.raw_path, check.status, check.normalized_path, check.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

pub(super) fn display_redacted_list(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values
            .iter()
            .map(|value| ledger::redact_text(value))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(super) fn display_owned_write_paths(values: &[(u32, String)]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values
            .iter()
            .map(|(lane, path)| format!("lane {lane}:{path}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(super) fn display_optional_lane(value: Option<u32>) -> String {
    value
        .map(|lane| lane.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

pub(super) fn display_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "없음".to_string())
}

pub(super) fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

pub(super) fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}
