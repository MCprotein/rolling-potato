use crate::foundation::error::AppError;

use super::manifest::ResolvedSkillManifest;
use super::policy::validate_required;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillState {
    Selected,
    ContextReady,
    ModelRequested,
    ActionRecorded,
    AwaitingApproval,
    AwaitingVerification,
    StopPassed,
    Complete,
    Failed,
    Cancelled,
}

impl SkillState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::ContextReady => "context-ready",
            Self::ModelRequested => "model-requested",
            Self::ActionRecorded => "action-recorded",
            Self::AwaitingApproval => "awaiting-approval",
            Self::AwaitingVerification => "awaiting-verification",
            Self::StopPassed => "stop-passed",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "selected" => Self::Selected,
            "context-ready" => Self::ContextReady,
            "model-requested" => Self::ModelRequested,
            "action-recorded" => Self::ActionRecorded,
            "awaiting-approval" => Self::AwaitingApproval,
            "awaiting-verification" => Self::AwaitingVerification,
            "stop-passed" => Self::StopPassed,
            "complete" => Self::Complete,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRuntimeState {
    pub active_skill_id: String,
    pub invocation: String,
    pub state: SkillState,
    pub completed_hooks: Vec<String>,
    pub evidence: Vec<String>,
    pub completed_stop_criteria: Vec<String>,
}

impl SkillRuntimeState {
    pub fn new_resolved(
        manifest: &ResolvedSkillManifest,
        invocation: &str,
    ) -> Result<Self, AppError> {
        if !matches!(invocation, "explicit" | "natural-language") {
            return Err(AppError::blocked(format!(
                "skill invocation 차단\n- skill: {}\n- 이유: 알 수 없는 invocation source: {invocation}",
                manifest.id()
            )));
        }
        Ok(Self {
            active_skill_id: manifest.id().to_string(),
            invocation: invocation.to_string(),
            state: SkillState::Selected,
            completed_hooks: Vec::new(),
            evidence: Vec::new(),
            completed_stop_criteria: Vec::new(),
        })
    }

    pub fn transition(&mut self, next: SkillState) -> Result<(), AppError> {
        validate_transition(self.state, next)?;
        self.state = next;
        Ok(())
    }

    pub fn record_hook(&mut self, hook: &str) -> Result<(), AppError> {
        if !crate::runtime_core::extensions::hook::HOOK_POINTS
            .iter()
            .any(|point| point.name == hook)
        {
            return Err(AppError::blocked(format!(
                "skill hook 기록 차단\n- skill: {}\n- hook: {}\n- 이유: 등록되지 않은 hook point",
                self.active_skill_id, hook
            )));
        }
        push_unique(&mut self.completed_hooks, hook);
        Ok(())
    }

    pub fn record_evidence(&mut self, evidence: &str) {
        push_unique(&mut self.evidence, evidence);
    }

    pub fn record_stop_criterion(&mut self, criterion: &str) {
        push_unique(&mut self.completed_stop_criteria, criterion);
    }

    pub fn validate_stop_against(&self, manifest: &ResolvedSkillManifest) -> Result<(), AppError> {
        validate_required(
            manifest.id(),
            "hook",
            manifest.required_hooks(),
            &self.completed_hooks,
        )?;
        validate_required(
            manifest.id(),
            "evidence",
            manifest.evidence_requirements(),
            &self.evidence,
        )?;
        validate_required(
            manifest.id(),
            "stop criterion",
            manifest.stop_criteria(),
            &self.completed_stop_criteria,
        )
    }
}

pub fn validate_transition(current: SkillState, next: SkillState) -> Result<(), AppError> {
    let allowed = matches!(
        (current, next),
        (SkillState::Selected, SkillState::ContextReady)
            | (SkillState::ContextReady, SkillState::ModelRequested)
            | (SkillState::ModelRequested, SkillState::ActionRecorded)
            | (SkillState::ActionRecorded, SkillState::AwaitingApproval)
            | (SkillState::ActionRecorded, SkillState::StopPassed)
            | (
                SkillState::AwaitingApproval,
                SkillState::AwaitingVerification
            )
            | (SkillState::AwaitingVerification, SkillState::StopPassed)
            | (SkillState::StopPassed, SkillState::Complete)
    ) || (!current.is_terminal()
        && matches!(next, SkillState::Failed | SkillState::Cancelled));

    if allowed {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "skill state transition 차단\n- current: {}\n- next: {}",
            current.label(),
            next.label()
        )))
    }
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}
