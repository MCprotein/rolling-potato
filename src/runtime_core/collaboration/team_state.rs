//! Team manifest DTOs, persisted state DTO, and stage transition policy.

use crate::foundation::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMemberV1 {
    pub lane: u32,
    pub member_id: String,
    pub role: String,
    pub task: String,
    pub task_hash: String,
    pub tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamManifestV1 {
    pub team_id: String,
    pub parent_workflow_id: String,
    pub members: Vec<TeamMemberV1>,
    pub write_policy: String,
    pub merge_policy: String,
    pub stop_gate: String,
    pub artifact_hash: String,
    pub(crate) canonical_body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamStage {
    Plan,
    Dispatch,
    Execute,
    Review,
    Verify,
    Merge,
    Report,
    Complete,
    Failed,
    Cancelled,
}

impl TeamStage {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "team-plan" => Some(Self::Plan),
            "team-dispatch" => Some(Self::Dispatch),
            "team-exec" => Some(Self::Execute),
            "team-review" => Some(Self::Review),
            "team-verify" => Some(Self::Verify),
            "team-merge" => Some(Self::Merge),
            "team-report" => Some(Self::Report),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "team-plan",
            Self::Dispatch => "team-dispatch",
            Self::Execute => "team-exec",
            Self::Review => "team-review",
            Self::Verify => "team-verify",
            Self::Merge => "team-merge",
            Self::Report => "team-report",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled)
    }

    pub(crate) fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Plan, Self::Dispatch)
                | (Self::Dispatch, Self::Execute)
                | (Self::Execute, Self::Review)
                | (Self::Review, Self::Verify)
                | (Self::Verify, Self::Merge)
                | (Self::Merge, Self::Report)
                | (Self::Report, Self::Complete)
        ) || (!self.is_terminal() && matches!(next, Self::Failed | Self::Cancelled))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamStateV1 {
    pub team_id: String,
    pub revision: u64,
    pub previous_hash: String,
    pub artifact_hash: String,
    pub manifest_hash: String,
    pub project_id: String,
    pub session_id: String,
    pub parent_workflow_id: String,
    pub parent_revision: u64,
    pub parent_artifact_hash: String,
    pub stage: TeamStage,
    pub status: String,
    pub requested_lanes: u32,
    pub admitted_lanes: u32,
    pub execution_mode: String,
    pub member_count: u32,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

impl TeamStateV1 {
    pub(crate) fn transition_to_at(
        &mut self,
        next: TeamStage,
        admitted_lanes: Option<u32>,
        execution_mode: Option<&str>,
        updated_at_ms: u128,
    ) -> Result<(), AppError> {
        if !self.stage.permits(next) {
            return Err(AppError::blocked(format!(
                "team stage 전이 차단\n- current: {}\n- next: {}",
                self.stage.as_str(),
                next.as_str()
            )));
        }
        if next == TeamStage::Dispatch {
            let admitted_lanes = admitted_lanes.ok_or_else(|| {
                AppError::blocked("team dispatch stage에는 admitted lane 수가 필요합니다.")
            })?;
            if admitted_lanes == 0 || admitted_lanes > self.requested_lanes {
                return Err(AppError::blocked(
                    "team dispatch admitted lane binding이 요청 범위를 벗어났습니다.",
                ));
            }
            let execution_mode = execution_mode.unwrap_or("");
            if !matches!(execution_mode, "parallel" | "sequential") {
                return Err(AppError::blocked(
                    "team dispatch execution mode는 parallel 또는 sequential이어야 합니다.",
                ));
            }
            self.admitted_lanes = admitted_lanes;
            self.execution_mode = execution_mode.to_string();
        } else if admitted_lanes.is_some() || execution_mode.is_some() {
            return Err(AppError::blocked(
                "team dispatch 외 stage에서 admission binding을 변경할 수 없습니다.",
            ));
        }
        self.stage = next;
        self.status = match next {
            TeamStage::Complete => "completed",
            TeamStage::Failed => "failed",
            TeamStage::Cancelled => "cancelled",
            _ => "active",
        }
        .to_string();
        self.updated_at_ms = updated_at_ms;
        Ok(())
    }
}
