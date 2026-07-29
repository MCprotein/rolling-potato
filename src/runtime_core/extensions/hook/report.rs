use crate::foundation::error::AppError;

use super::codec::parse_hook_status;
use super::policy::{fail_closed, resolve_conflict, status_label};
use super::registry::{HOOK_LAYER_ORDER, HOOK_POINTS};
use super::types::{HookLayer, HookStatus};

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
