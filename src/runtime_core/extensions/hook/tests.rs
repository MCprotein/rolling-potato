use super::codec::parse_hook_status;
use super::policy::{dispatch, fail_closed, resolve_conflict};
use super::registry::HOOK_POINTS;
use super::types::{HookCapability, HookInput, HookLayer, HookRule, HookStatus};

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
