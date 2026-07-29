//! Policy decision value objects and path-policy port contract.

use std::path::{Path, PathBuf};

use crate::foundation::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSource {
    User,
    Project,
    Local,
    Session,
    Policy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    ReadFile,
    WriteFile,
    RunCommand,
    ApplyPatch,
    NetworkDownload,
    PluginCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionStatus {
    Create,
    Update,
    Noop,
    UserModified,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    pub decision: Decision,
    pub action_kind: ActionKind,
    pub rule_source: RuleSource,
    pub command_class: &'static str,
    pub reason: String,
    pub approval_prompt: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub display: String,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    Read,
    Write,
}

pub(crate) trait PathPolicyPort {
    fn canonical_project_root(&self) -> Result<PathBuf, AppError>;

    fn normalize_existing_or_parent(&self, path: &Path) -> Result<PathBuf, AppError>;
}

impl PolicyDecision {
    pub(super) fn new(
        decision: Decision,
        action_kind: ActionKind,
        command_class: &'static str,
        reason: impl Into<String>,
        approval_prompt: &'static str,
    ) -> Self {
        Self {
            decision,
            action_kind,
            rule_source: RuleSource::Policy,
            command_class,
            reason: reason.into(),
            approval_prompt,
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self.decision {
            Decision::Allow => "allow",
            Decision::Ask => "ask",
            Decision::Deny => "deny",
        }
    }
}
