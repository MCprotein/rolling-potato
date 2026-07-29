use crate::foundation::serialization as strict_json;

use super::types::HookStatus;

pub(super) fn parse_hook_status(json: &str) -> HookStatus {
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
