use super::types::HookLayer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HookPoint {
    pub(crate) name: &'static str,
    pub(crate) phase: &'static str,
}

pub(super) const HOOK_LAYER_ORDER: &[HookLayer] = &[
    HookLayer::Runtime,
    HookLayer::Project,
    HookLayer::Skill,
    HookLayer::Session,
    HookLayer::Observer,
];

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
