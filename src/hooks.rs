use crate::app::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookStatus {
    Observe,
    Allow,
    Modify,
    Ask,
    Deny,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookPoint {
    pub name: &'static str,
    pub phase: &'static str,
}

pub const HOOK_POINTS: &[HookPoint] = &[
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

pub fn list_report() -> String {
    let rows = HOOK_POINTS
        .iter()
        .map(|hook| format!("- {} | phase: {}", hook.name, hook.phase))
        .collect::<Vec<_>>()
        .join("\n");
    let sample_conflict = resolve_conflict(&[HookStatus::Allow, HookStatus::Ask]);

    format!(
        "hook registry\n- hooks: {}\n- ordering: runtime -> project -> skill -> session -> observer\n- conflict rule: error/deny > ask > modify > allow > observe\n- sample conflict allow+ask: {}\n- fail closed: unknown/error hook result는 deny로 취급\n- input schema: hook, session_id, workflow_id, project_root, mode, active_skill_id, actor_id, payload, evidence_pointer, policy_context\n- output schema: status, modified_payload, reason_ko, evidence_record, ledger_metadata\n{}",
        HOOK_POINTS.len(),
        status_label(sample_conflict),
        rows
    )
}

pub fn validate_result_report(json: &str) -> Result<String, AppError> {
    let status = parse_hook_status(json);
    let resolved = fail_closed(status);
    Ok(format!(
        "hook result 검사\n- parsed status: {}\n- resolved status: {}\n- 동작: unknown/error result는 fail-closed로 deny 처리합니다.",
        status_label(status),
        status_label(resolved)
    ))
}

pub fn parse_hook_status(json: &str) -> HookStatus {
    let lower = json.to_ascii_lowercase();
    if lower.contains("\"status\":\"deny\"") || lower.contains("\"status\": \"deny\"") {
        HookStatus::Deny
    } else if lower.contains("\"status\":\"ask\"") || lower.contains("\"status\": \"ask\"") {
        HookStatus::Ask
    } else if lower.contains("\"status\":\"modify\"") || lower.contains("\"status\": \"modify\"") {
        HookStatus::Modify
    } else if lower.contains("\"status\":\"allow\"") || lower.contains("\"status\": \"allow\"") {
        HookStatus::Allow
    } else if lower.contains("\"status\":\"observe\"") || lower.contains("\"status\": \"observe\"")
    {
        HookStatus::Observe
    } else {
        HookStatus::Error
    }
}

pub fn resolve_conflict(statuses: &[HookStatus]) -> HookStatus {
    statuses
        .iter()
        .copied()
        .map(fail_closed)
        .max_by_key(|status| status_rank(*status))
        .unwrap_or(HookStatus::Observe)
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

fn status_label(status: HookStatus) -> &'static str {
    match status {
        HookStatus::Observe => "observe",
        HookStatus::Allow => "allow",
        HookStatus::Modify => "modify",
        HookStatus::Ask => "ask",
        HookStatus::Deny => "deny",
        HookStatus::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_required_hook_points() {
        assert!(HOOK_POINTS.iter().any(|hook| hook.name == "pre_tool_call"));
        assert!(HOOK_POINTS.iter().any(|hook| hook.name == "stop_gate"));
    }

    #[test]
    fn deny_wins_hook_conflict() {
        let status = resolve_conflict(&[HookStatus::Allow, HookStatus::Deny]);
        assert_eq!(status, HookStatus::Deny);
    }

    #[test]
    fn unknown_hook_result_fails_closed() {
        let status = parse_hook_status(r#"{"status":"wat"}"#);
        assert_eq!(fail_closed(status), HookStatus::Deny);
    }
}
