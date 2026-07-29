//! Human-readable policy schema reporting.

use super::types::{ActionKind, ActionStatus, RuleSource};

const ALL_RULE_SOURCES: &[RuleSource] = &[
    RuleSource::User,
    RuleSource::Project,
    RuleSource::Local,
    RuleSource::Session,
    RuleSource::Policy,
];

const ALL_ACTION_KINDS: &[ActionKind] = &[
    ActionKind::ReadFile,
    ActionKind::WriteFile,
    ActionKind::RunCommand,
    ActionKind::ApplyPatch,
    ActionKind::NetworkDownload,
    ActionKind::PluginCapability,
];

const ALL_ACTION_STATUSES: &[ActionStatus] = &[
    ActionStatus::Create,
    ActionStatus::Update,
    ActionStatus::Noop,
    ActionStatus::UserModified,
    ActionStatus::Blocked,
];

pub fn schema_report() -> String {
    format!(
        "policy schema\n- action kinds: {}\n- decisions: allow, ask, deny\n- rule sources: {}\n- action status: {}\n- write policy: diff-before-write + approval required\n- user-modified policy: owned region 외 변경은 blocked 또는 ask\n- managed artifact policy: manifest/hash tracking required before install/download\n- network policy: download and remote connector require ask\n- destructive command policy: deny or high-confirm by default",
        ALL_ACTION_KINDS
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        ALL_RULE_SOURCES
            .iter()
            .map(|source| format!("{source:?}"))
            .collect::<Vec<_>>()
            .join(", "),
        ALL_ACTION_STATUSES
            .iter()
            .map(|status| format!("{status:?}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
