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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HookCapability {
    Observe,
    ModifyPayload,
    ExecuteCommand,
    WriteFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookRule {
    pub(crate) id: String,
    pub(crate) layer: HookLayer,
    pub(crate) status: HookStatus,
    pub(crate) modified_payload: Option<String>,
    pub(crate) reason: String,
    pub(super) capabilities: Vec<HookCapability>,
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
