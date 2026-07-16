//! Deterministic, side-effect-free hook policy evaluation.

use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookStatus {
    Observe,
    Allow,
    Modify,
    Ask,
    Deny,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HookLayer {
    Runtime,
    Project,
    Skill,
    Session,
    Observer,
}

const HOOK_LAYER_ORDER: &[HookLayer] = &[
    HookLayer::Runtime,
    HookLayer::Project,
    HookLayer::Skill,
    HookLayer::Session,
    HookLayer::Observer,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookCapability {
    Observe,
    ModifyPayload,
    ExecuteCommand,
    WriteFile,
}

const FORBIDDEN_HOOK_CAPABILITIES: &[HookCapability] =
    &[HookCapability::ExecuteCommand, HookCapability::WriteFile];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HookPoint {
    pub(crate) name: &'static str,
    pub(crate) phase: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookRule {
    pub(crate) id: String,
    pub(crate) layer: HookLayer,
    pub(crate) status: HookStatus,
    pub(crate) modified_payload: Option<String>,
    pub(crate) reason: String,
    pub(crate) capabilities: Vec<HookCapability>,
}

impl HookRule {
    pub(crate) fn decision(
        id: impl Into<String>,
        layer: HookLayer,
        status: HookStatus,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            layer,
            status,
            modified_payload: None,
            reason: reason.into(),
            capabilities: vec![HookCapability::Observe],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HookInput<'a> {
    pub(crate) hook: &'a str,
    pub(crate) workflow_id: Option<&'a str>,
    pub(crate) active_skill_id: Option<&'a str>,
    pub(crate) mode: &'a str,
    pub(crate) payload: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookDispatch {
    pub(crate) status: HookStatus,
    pub(crate) payload: String,
    pub(crate) ordered_rule_ids: Vec<String>,
    pub(crate) reasons: Vec<String>,
    pub(crate) ledger_event_id: Option<String>,
}

pub(crate) const HOOK_POINTS: &[HookPoint] = &[
    HookPoint {
        name: "session_start",
        phase: "session",
    },
    HookPoint {
        name: "user_request_received",
        phase: "session",
    },
    HookPoint {
        name: "pre_context_pack",
        phase: "context",
    },
    HookPoint {
        name: "post_context_pack",
        phase: "context",
    },
    HookPoint {
        name: "pre_model_request",
        phase: "model",
    },
    HookPoint {
        name: "post_model_response",
        phase: "model",
    },
    HookPoint {
        name: "pre_action_parse",
        phase: "action",
    },
    HookPoint {
        name: "post_action_parse",
        phase: "action",
    },
    HookPoint {
        name: "pre_tool_call",
        phase: "tool",
    },
    HookPoint {
        name: "post_tool_result",
        phase: "tool",
    },
    HookPoint {
        name: "pre_patch_apply",
        phase: "patch",
    },
    HookPoint {
        name: "post_patch_apply",
        phase: "patch",
    },
    HookPoint {
        name: "pre_command_run",
        phase: "command",
    },
    HookPoint {
        name: "post_command_run",
        phase: "command",
    },
    HookPoint {
        name: "pre_final_report",
        phase: "report",
    },
    HookPoint {
        name: "stop_gate",
        phase: "verification",
    },
    HookPoint {
        name: "session_end",
        phase: "session",
    },
];

pub(crate) fn list_report() -> String {
    let rows = HOOK_POINTS
        .iter()
        .map(|hook| format!("- {} | phase: {}", hook.name, hook.phase))
        .collect::<Vec<_>>()
        .join("\n");
    let sample_conflict = resolve_conflict(&[HookStatus::Allow, HookStatus::Ask]);
    let ordering = HOOK_LAYER_ORDER
        .iter()
        .map(|layer| match layer {
            HookLayer::Runtime => "runtime",
            HookLayer::Project => "project",
            HookLayer::Skill => "skill",
            HookLayer::Session => "session",
            HookLayer::Observer => "observer",
        })
        .collect::<Vec<_>>()
        .join(" -> ");

    format!(
        "hook registry\n- hooks: {}\n- ordering: {}\n- conflict rule: error/deny > ask > modify > allow > observe\n- sample conflict allow+ask: {}\n- fail closed: unknown/error hook result는 deny로 취급\n- side-effect boundary: hook의 direct command/file write는 금지\n- input schema: hook, session_id, workflow_id, project_root, mode, active_skill_id, actor_id, payload, evidence_pointer, policy_context\n- output schema: status, modified_payload, reason_ko, evidence_record, ledger_metadata\n{}",
        HOOK_POINTS.len(),
        ordering,
        status_label(sample_conflict),
        rows
    )
}

pub(crate) fn validate_result_report(json: &str) -> Result<String, AppError> {
    let status = parse_hook_status(json);
    let resolved = fail_closed(status);
    Ok(format!(
        "hook result 검사\n- parsed status: {}\n- resolved status: {}\n- 동작: unknown/error result는 fail-closed로 deny 처리합니다.",
        status_label(status),
        status_label(resolved)
    ))
}

pub(crate) fn parse_hook_status(json: &str) -> HookStatus {
    let Ok(object) = strict_json::parse_object(
        json,
        &[
            "status",
            "modified_payload",
            "reason_ko",
            "evidence_record",
            "ledger_metadata",
        ],
        "hook-result",
    ) else {
        return HookStatus::Error;
    };
    let Ok(status) = strict_json::string(&object, "status", "hook-result") else {
        return HookStatus::Error;
    };
    match status.as_str() {
        "observe" => HookStatus::Observe,
        "allow" => HookStatus::Allow,
        "modify" => HookStatus::Modify,
        "ask" => HookStatus::Ask,
        "deny" => HookStatus::Deny,
        "error" => HookStatus::Error,
        _ => HookStatus::Error,
    }
}

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

pub(crate) fn resolve_conflict(statuses: &[HookStatus]) -> HookStatus {
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

fn fail_closed(status: HookStatus) -> HookStatus {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>(hook: &'a str, payload: &'a str) -> HookInput<'a> {
        HookInput {
            hook,
            workflow_id: Some("wf-test"),
            active_skill_id: Some("small-patch"),
            mode: "execute",
            payload,
        }
    }

    #[test]
    fn registry_contains_required_hook_points() {
        assert_eq!(HOOK_POINTS.len(), 17);
        assert!(HOOK_POINTS.iter().any(|hook| hook.name == "pre_tool_call"));
        assert!(HOOK_POINTS.iter().any(|hook| hook.name == "stop_gate"));
    }

    #[test]
    fn dispatch_uses_layer_then_id_order() {
        let rules = vec![
            HookRule::decision("z-observer", HookLayer::Observer, HookStatus::Observe, ""),
            HookRule::decision("b-skill", HookLayer::Skill, HookStatus::Allow, ""),
            HookRule::decision("a-skill", HookLayer::Skill, HookStatus::Allow, ""),
            HookRule::decision("runtime", HookLayer::Runtime, HookStatus::Allow, ""),
            HookRule::decision("project", HookLayer::Project, HookStatus::Allow, ""),
            HookRule::decision("session", HookLayer::Session, HookStatus::Allow, ""),
        ];

        let result = dispatch(input("pre_tool_call", "read_file"), &rules);

        assert_eq!(
            result.ordered_rule_ids,
            [
                "runtime",
                "project",
                "a-skill",
                "b-skill",
                "session",
                "z-observer"
            ]
        );
    }

    #[test]
    fn deny_and_ask_win_hook_conflicts() {
        assert_eq!(
            resolve_conflict(&[HookStatus::Allow, HookStatus::Ask]),
            HookStatus::Ask
        );
        assert_eq!(
            resolve_conflict(&[HookStatus::Allow, HookStatus::Ask, HookStatus::Deny]),
            HookStatus::Deny
        );
    }

    #[test]
    fn malformed_unknown_and_error_results_fail_closed() {
        for raw in [
            r#"{"status":"wat"}"#,
            r#"{"status":"allow","unknown":true}"#,
            r#"{"status":"allow""#,
            r#"{"status":"error"}"#,
        ] {
            assert_eq!(fail_closed(parse_hook_status(raw)), HookStatus::Deny);
        }
    }

    #[test]
    fn modifications_are_applied_in_deterministic_order() {
        let rules = vec![
            modification("project", HookLayer::Project, "project", "project rewrite"),
            modification("runtime", HookLayer::Runtime, "runtime", "runtime rewrite"),
        ];

        let result = dispatch(input("pre_context_pack", "original"), &rules);

        assert_eq!(result.status, HookStatus::Modify);
        assert_eq!(result.payload, "project");
        assert_eq!(result.ordered_rule_ids, ["runtime", "project"]);
    }

    #[test]
    fn runtime_deny_cannot_be_widened_by_skill_allow() {
        let rules = vec![
            HookRule::decision(
                "runtime",
                HookLayer::Runtime,
                HookStatus::Deny,
                "policy denied",
            ),
            HookRule::decision(
                "skill",
                HookLayer::Skill,
                HookStatus::Allow,
                "skill allowed",
            ),
        ];

        assert_eq!(
            dispatch(input("pre_tool_call", "apply_patch"), &rules).status,
            HookStatus::Deny
        );
    }

    #[test]
    fn direct_command_or_file_write_capability_is_rejected() {
        for capability in [HookCapability::ExecuteCommand, HookCapability::WriteFile] {
            let mut rule =
                HookRule::decision("unsafe", HookLayer::Project, HookStatus::Allow, "unsafe");
            rule.capabilities = vec![capability];

            assert_eq!(
                dispatch(input("pre_tool_call", "ignored"), &[rule]).status,
                HookStatus::Deny
            );
        }
    }

    #[test]
    fn unknown_hook_point_is_denied() {
        let result = dispatch(input("not_registered", "payload"), &[]);

        assert_eq!(result.status, HookStatus::Deny);
        assert_eq!(result.ordered_rule_ids, ["runtime.unknown-hook"]);
    }

    fn modification(id: &str, layer: HookLayer, payload: &str, reason: &str) -> HookRule {
        HookRule {
            id: id.to_string(),
            layer,
            status: HookStatus::Modify,
            modified_payload: Some(payload.to_string()),
            reason: reason.to_string(),
            capabilities: vec![HookCapability::ModifyPayload],
        }
    }
}
