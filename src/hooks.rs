//! Concrete hook lifecycle adapters for state, ledger, and skill resolution.

use crate::foundation::error::AppError;
use crate::state;

pub(crate) use crate::runtime_core::extensions::hook::{
    dispatch, list_report, status_label, validate_result_report, HookDispatch, HookInput,
    HookLayer, HookRule, HookStatus,
};

pub(crate) fn dispatch_and_record(
    input: HookInput<'_>,
    rules: &[HookRule],
) -> Result<HookDispatch, AppError> {
    let mut result = dispatch(input, rules);
    let details = dispatch_ledger_details(input, &result);
    let event_id = state::record_event(
        "hook.dispatched",
        &format!("{} lifecycle hook 처리", input.hook),
        &details,
    )?;
    result.ledger_event_id = Some(event_id);
    validate_dispatch_result(input, &result)?;
    Ok(result)
}

pub(crate) fn dispatch_native_lifecycle(
    input: HookInput<'_>,
    tool: Option<&str>,
) -> Result<HookDispatch, AppError> {
    let rules = native_lifecycle_rules(input, tool);
    dispatch_and_record(input, &rules)
}

pub(crate) fn dispatch_native_lifecycle_for_skill(
    input: HookInput<'_>,
    tool: Option<&str>,
    skill: &crate::skill::ResolvedSkillManifest,
) -> Result<HookDispatch, AppError> {
    let rules = native_lifecycle_rules_for_skill(input, tool, skill);
    dispatch_and_record(input, &rules)
}

pub(crate) fn prepare_native_lifecycle_event(
    input: HookInput<'_>,
    tool: Option<&str>,
    identity: &crate::ledger::RuntimeIdentity,
) -> Result<(HookDispatch, crate::ledger::LedgerEvent), AppError> {
    let rules = native_lifecycle_rules(input, tool);
    let mut result = dispatch(input, &rules);
    validate_dispatch_result(input, &result)?;
    let event = crate::ledger::new_event_for(
        identity,
        "hook.dispatched",
        &format!("{} lifecycle hook 처리", input.hook),
        &dispatch_ledger_details(input, &result),
    );
    result.ledger_event_id = Some(event.event_id.clone());
    Ok((result, event))
}

pub(crate) fn validate_prepared_native_lifecycle_event(
    input: HookInput<'_>,
    tool: Option<&str>,
    identity: &crate::ledger::RuntimeIdentity,
    event: &crate::ledger::LedgerEvent,
) -> Result<(), AppError> {
    let rules = native_lifecycle_rules(input, tool);
    let result = dispatch(input, &rules);
    validate_dispatch_result(input, &result)?;
    let expected_summary = format!("{} lifecycle hook 처리", input.hook);
    let expected_details = dispatch_ledger_details(input, &result);
    if event.event_type != "hook.dispatched"
        || event.project_id != identity.project_id
        || event.session_id != identity.session_id
        || event.summary != expected_summary
        || event.details != expected_details
    {
        return Err(AppError::blocked(
            "prepared native lifecycle event semantic binding 불일치",
        ));
    }
    Ok(())
}

fn native_lifecycle_rules(input: HookInput<'_>, tool: Option<&str>) -> Vec<HookRule> {
    let mut rules = vec![runtime_rule()];
    if let Some(skill_id) = input.active_skill_id {
        let skill_rule = match crate::skill::resolve_skill(skill_id) {
            Ok(Some(skill)) => resolved_skill_rule(input, tool, &skill),
            Ok(None) => HookRule::decision(
                format!("skill.{skill_id}"),
                HookLayer::Skill,
                HookStatus::Deny,
                "skill manifest not found",
            ),
            Err(error) => HookRule::decision(
                format!("skill.{skill_id}"),
                HookLayer::Skill,
                HookStatus::Deny,
                error.message,
            ),
        };
        rules.push(skill_rule);
    }
    rules.push(observer_rule());
    rules
}

fn native_lifecycle_rules_for_skill(
    input: HookInput<'_>,
    tool: Option<&str>,
    skill: &crate::skill::ResolvedSkillManifest,
) -> Vec<HookRule> {
    let mut rules = vec![runtime_rule()];
    if input.active_skill_id.is_some() {
        rules.push(resolved_skill_rule(input, tool, skill));
    }
    rules.push(observer_rule());
    rules
}

fn resolved_skill_rule(
    input: HookInput<'_>,
    tool: Option<&str>,
    skill: &crate::skill::ResolvedSkillManifest,
) -> HookRule {
    let skill_id = input.active_skill_id.expect("caller checks active skill");
    if skill_id != skill.id() {
        return HookRule::decision(
            format!("skill.{skill_id}"),
            HookLayer::Skill,
            HookStatus::Deny,
            format!("resolved skill binding mismatch: {}", skill.id()),
        );
    }

    match tool {
        Some(tool) => match crate::skill::enforce_resolved_tool(skill, tool) {
            Ok(()) => HookRule::decision(
                format!("skill.{}", skill.id()),
                HookLayer::Skill,
                HookStatus::Allow,
                format!("tool allowed: {tool}"),
            ),
            Err(error) => HookRule::decision(
                format!("skill.{}", skill.id()),
                HookLayer::Skill,
                HookStatus::Deny,
                error.message,
            ),
        },
        None => HookRule::decision(
            format!("skill.{}", skill.id()),
            HookLayer::Skill,
            HookStatus::Allow,
            "required lifecycle hook",
        ),
    }
}

fn runtime_rule() -> HookRule {
    HookRule::decision(
        "runtime.lifecycle",
        HookLayer::Runtime,
        HookStatus::Allow,
        "registered native lifecycle point",
    )
}

fn observer_rule() -> HookRule {
    HookRule::decision(
        "observer.ledger",
        HookLayer::Observer,
        HookStatus::Observe,
        "ledger projection enabled",
    )
}

fn validate_dispatch_result(input: HookInput<'_>, result: &HookDispatch) -> Result<(), AppError> {
    match result.status {
        HookStatus::Deny | HookStatus::Error => Err(AppError::blocked(format!(
            "hook 실행 차단\n- hook: {}\n- status: {}\n- rules: {}\n- 이유: {}",
            input.hook,
            status_label(result.status),
            result.ordered_rule_ids.join(","),
            result.reasons.join(" | ")
        ))),
        HookStatus::Ask => Err(AppError::blocked(format!(
            "hook 승인 필요\n- hook: {}\n- rules: {}\n- 이유: {}",
            input.hook,
            result.ordered_rule_ids.join(","),
            result.reasons.join(" | ")
        ))),
        HookStatus::Observe | HookStatus::Allow | HookStatus::Modify => Ok(()),
    }
}

fn dispatch_ledger_details(input: HookInput<'_>, result: &HookDispatch) -> String {
    let payload_hash = state::sha256_text(&result.payload);
    format!(
        "hook={} workflow_id={} active_skill_id={} mode={} status={} ordered_rules={} payload_sha256={}",
        input.hook,
        input.workflow_id.unwrap_or("none"),
        input.active_skill_id.unwrap_or("none"),
        input.mode,
        status_label(result.status),
        result.ordered_rule_ids.join(","),
        payload_hash
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_details_hash_modified_payload_without_embedding_it() {
        let input = HookInput {
            hook: "pre_context_pack",
            workflow_id: Some("wf-test"),
            active_skill_id: Some("small-patch"),
            mode: "execute",
            payload: "original",
        };
        let result = HookDispatch {
            status: HookStatus::Modify,
            payload: "project rewrite".to_string(),
            ordered_rule_ids: vec!["runtime".to_string(), "project".to_string()],
            reasons: Vec::new(),
            ledger_event_id: None,
        };

        let details = dispatch_ledger_details(input, &result);

        assert!(details.contains("payload_sha256="));
        assert!(!details.contains("project rewrite"));
    }
}
