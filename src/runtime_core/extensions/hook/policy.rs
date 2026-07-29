use super::registry::HOOK_POINTS;
use super::types::{HookCapability, HookDispatch, HookInput, HookRule, HookStatus};

const FORBIDDEN_HOOK_CAPABILITIES: &[HookCapability] =
    &[HookCapability::ExecuteCommand, HookCapability::WriteFile];

pub(crate) fn dispatch(input: HookInput<'_>, rules: &[HookRule]) -> HookDispatch {
    if !HOOK_POINTS.iter().any(|point| point.name == input.hook) {
        return denied_dispatch(
            input.payload,
            "runtime.unknown-hook",
            format!("등록되지 않은 hook point: {}", input.hook),
        );
    }

    let mut ordered = rules.to_vec();
    ordered.sort_by(|left, right| {
        left.layer
            .cmp(&right.layer)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut payload = input.payload.to_string();
    let mut statuses = Vec::with_capacity(ordered.len());
    let mut ordered_rule_ids = Vec::with_capacity(ordered.len());
    let mut reasons = Vec::new();

    for rule in ordered {
        ordered_rule_ids.push(rule.id.clone());
        if rule
            .capabilities
            .iter()
            .any(|capability| FORBIDDEN_HOOK_CAPABILITIES.contains(capability))
        {
            statuses.push(HookStatus::Deny);
            reasons.push(format!(
                "{}: hook direct command/file write capability 차단",
                rule.id
            ));
            continue;
        }

        if rule.status == HookStatus::Modify {
            match rule.modified_payload {
                Some(modified) if rule.capabilities.contains(&HookCapability::ModifyPayload) => {
                    payload = modified;
                }
                _ => {
                    statuses.push(HookStatus::Deny);
                    reasons.push(format!("{}: 유효하지 않은 payload modification", rule.id));
                    continue;
                }
            }
        } else if rule.modified_payload.is_some() {
            statuses.push(HookStatus::Deny);
            reasons.push(format!(
                "{}: modify 이외 status의 payload 변경 차단",
                rule.id
            ));
            continue;
        }

        statuses.push(rule.status);
        if !rule.reason.is_empty() {
            reasons.push(format!("{}: {}", rule.id, rule.reason));
        }
    }

    HookDispatch {
        status: resolve_conflict(&statuses),
        payload,
        ordered_rule_ids,
        reasons,
        ledger_event_id: None,
    }
}

pub(super) fn resolve_conflict(statuses: &[HookStatus]) -> HookStatus {
    statuses
        .iter()
        .copied()
        .map(fail_closed)
        .max_by_key(|status| status_rank(*status))
        .unwrap_or(HookStatus::Observe)
}

pub(crate) fn status_label(status: HookStatus) -> &'static str {
    match status {
        HookStatus::Observe => "observe",
        HookStatus::Allow => "allow",
        HookStatus::Modify => "modify",
        HookStatus::Ask => "ask",
        HookStatus::Deny => "deny",
        HookStatus::Error => "error",
    }
}

pub(super) fn fail_closed(status: HookStatus) -> HookStatus {
    match status {
        HookStatus::Error => HookStatus::Deny,
        other => other,
    }
}

fn status_rank(status: HookStatus) -> u8 {
    match status {
        HookStatus::Observe => 0,
        HookStatus::Allow => 1,
        HookStatus::Modify => 2,
        HookStatus::Ask => 3,
        HookStatus::Deny | HookStatus::Error => 4,
    }
}

fn denied_dispatch(payload: &str, id: &str, reason: String) -> HookDispatch {
    HookDispatch {
        status: HookStatus::Deny,
        payload: payload.to_string(),
        ordered_rule_ids: vec![id.to_string()],
        reasons: vec![reason],
        ledger_event_id: None,
    }
}
