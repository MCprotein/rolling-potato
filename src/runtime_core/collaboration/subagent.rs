//! Subagent role, tool, launch, and lifecycle policy.

use crate::foundation::error::AppError;
use crate::foundation::integrity;
pub(crate) use crate::runtime_core::inference::backend::MAX_CHAT_TIMEOUT_MS;

mod launch;
mod record_codec;

pub use launch::validate_launch;
pub(crate) use launch::{normalize_paths, normalize_relative_path, normalize_tools};
pub(crate) use record_codec::{parse_record, render_payload, render_record};

pub(crate) const MAX_RECORD_REVISIONS: u64 = 4;
pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;
pub const DEFAULT_MAX_TOKENS: u32 = 256;
pub const MAX_MAX_TOKENS: u32 = 1_024;
pub const MAX_TASK_BYTES: usize = 4_096;
pub const MAX_DECLARED_PATHS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubagentRole {
    Explore,
    Planner,
    Verifier,
    Critic,
    Writer,
    Executor,
}

impl SubagentRole {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "explore" => Some(Self::Explore),
            "planner" => Some(Self::Planner),
            "verifier" => Some(Self::Verifier),
            "critic" => Some(Self::Critic),
            "writer" => Some(Self::Writer),
            "executor" => Some(Self::Executor),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Planner => "planner",
            Self::Verifier => "verifier",
            Self::Critic => "critic",
            Self::Writer => "writer",
            Self::Executor => "executor",
        }
    }

    fn allows(self, tool: SubagentTool) -> bool {
        tool == SubagentTool::ReadFile
            || (self == Self::Executor && tool == SubagentTool::RenderDiff)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubagentTool {
    ReadFile,
    RenderDiff,
}

impl SubagentTool {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read_file" => Some(Self::ReadFile),
            "render_diff" => Some(Self::RenderDiff),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadFile => "read_file",
            Self::RenderDiff => "render_diff",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    Requested,
    Admitted,
    Running,
    Completed,
    Blocked,
    Failed,
    Cancelled,
    TimedOut,
}

impl SubagentStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "admitted" => Some(Self::Admitted),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "blocked" => Some(Self::Blocked),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "timed-out" => Some(Self::TimedOut),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Admitted => "admitted",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed-out",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Blocked | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }

    pub(crate) fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Requested,
                Self::Admitted | Self::Blocked | Self::Cancelled
            ) | (Self::Admitted, Self::Running | Self::Cancelled)
                | (
                    Self::Running,
                    Self::Completed
                        | Self::Blocked
                        | Self::Failed
                        | Self::Cancelled
                        | Self::TimedOut
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedLaunch {
    pub role: SubagentRole,
    pub task_hash: String,
    pub declared_tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub requested_max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentRecordV1 {
    pub subagent_id: String,
    pub revision: u64,
    pub previous_hash: String,
    pub artifact_hash: String,
    pub project_id: String,
    pub session_id: String,
    pub parent_workflow_id: String,
    pub parent_revision: u64,
    pub parent_artifact_hash: String,
    pub role: SubagentRole,
    pub task_hash: String,
    pub declared_tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub requested_max_tokens: u32,
    pub effective_max_tokens: u32,
    pub status: SubagentStatus,
    pub backend_event_id: String,
    pub result_artifact_id: String,
    pub result_artifact_hash: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    pub failure_code: String,
    pub created_at_ms: u128,
    pub started_at_ms: u128,
    pub finished_at_ms: u128,
}

pub(crate) struct NewRecordBinding<'a> {
    pub subagent_id: String,
    pub project_id: &'a str,
    pub session_id: &'a str,
    pub parent_workflow_id: &'a str,
    pub parent_revision: u64,
    pub parent_artifact_hash: &'a str,
    pub created_at_ms: u128,
}

impl SubagentRecordV1 {
    pub(crate) fn transition_to_at(
        &mut self,
        next: SubagentStatus,
        failure_code: Option<&str>,
        timestamp: u128,
    ) -> Result<(), AppError> {
        if !self.status.permits(next) {
            return Err(AppError::blocked(format!(
                "subagent 상태 전이 차단\n- current: {}\n- next: {}",
                self.status.as_str(),
                next.as_str()
            )));
        }
        if next == SubagentStatus::Running {
            self.started_at_ms = timestamp;
        }
        if next.is_terminal() {
            self.finished_at_ms = timestamp.max(self.started_at_ms);
        }
        self.failure_code = failure_code.unwrap_or("").trim().to_string();
        self.status = next;
        Ok(())
    }
}

pub(crate) fn create_record_at(
    binding: NewRecordBinding<'_>,
    launch: ValidatedLaunch,
) -> Result<SubagentRecordV1, AppError> {
    let record = SubagentRecordV1 {
        subagent_id: binding.subagent_id,
        revision: 0,
        previous_hash: String::new(),
        artifact_hash: String::new(),
        project_id: binding.project_id.to_string(),
        session_id: binding.session_id.to_string(),
        parent_workflow_id: binding.parent_workflow_id.to_string(),
        parent_revision: binding.parent_revision,
        parent_artifact_hash: binding.parent_artifact_hash.to_string(),
        role: launch.role,
        task_hash: launch.task_hash,
        declared_tools: launch.declared_tools,
        read_paths: launch.read_paths,
        write_paths: launch.write_paths,
        timeout_ms: launch.timeout_ms,
        requested_max_tokens: launch.requested_max_tokens,
        effective_max_tokens: launch.requested_max_tokens,
        status: SubagentStatus::Requested,
        backend_event_id: String::new(),
        result_artifact_id: String::new(),
        result_artifact_hash: String::new(),
        evidence_id: String::new(),
        evidence_hash: String::new(),
        failure_code: String::new(),
        created_at_ms: binding.created_at_ms,
        started_at_ms: 0,
        finished_at_ms: 0,
    };
    validate_record(&record, false)?;
    Ok(record)
}

pub(crate) fn validate_record(record: &SubagentRecordV1, installed: bool) -> Result<(), AppError> {
    validate_subagent_id(&record.subagent_id)?;
    for (label, value) in [
        ("project_id", record.project_id.as_str()),
        ("session_id", record.session_id.as_str()),
        ("parent_workflow_id", record.parent_workflow_id.as_str()),
    ] {
        if value.is_empty() || value.len() > 160 {
            return Err(AppError::blocked(format!("subagent {label} 범위 오류")));
        }
    }
    if record.parent_revision == 0 || !is_sha256(&record.parent_artifact_hash) {
        return Err(AppError::blocked("subagent parent binding 오류"));
    }
    if !is_sha256(&record.task_hash) {
        return Err(AppError::blocked("subagent task hash 오류"));
    }
    if record.timeout_ms == 0 || record.timeout_ms > MAX_CHAT_TIMEOUT_MS {
        return Err(AppError::blocked("subagent timeout state 오류"));
    }
    if record.requested_max_tokens == 0
        || record.requested_max_tokens > MAX_MAX_TOKENS
        || record.effective_max_tokens == 0
        || record.effective_max_tokens > record.requested_max_tokens
    {
        return Err(AppError::blocked("subagent token state 오류"));
    }
    validate_stored_tools_and_paths(record)?;
    if record.created_at_ms == 0 {
        return Err(AppError::blocked("subagent created timestamp 누락"));
    }
    match record.status {
        SubagentStatus::Requested | SubagentStatus::Admitted => {
            if record.started_at_ms != 0
                || record.finished_at_ms != 0
                || !record.backend_event_id.is_empty()
                || has_result_binding(record)
                || !record.failure_code.is_empty()
            {
                return Err(AppError::blocked("subagent pre-run timestamp 오류"));
            }
        }
        SubagentStatus::Running => {
            if record.started_at_ms == 0
                || record.finished_at_ms != 0
                || has_result_binding(record)
                || !record.failure_code.is_empty()
            {
                return Err(AppError::blocked("subagent running timestamp 오류"));
            }
        }
        status if status.is_terminal() => {
            if record.finished_at_ms == 0
                || (record.started_at_ms != 0 && record.finished_at_ms < record.started_at_ms)
            {
                return Err(AppError::blocked("subagent terminal timestamp 오류"));
            }
            if status == SubagentStatus::Completed {
                if record.backend_event_id.is_empty()
                    || record.result_artifact_id.is_empty()
                    || !is_sha256(&record.result_artifact_hash)
                    || record.evidence_id.is_empty()
                    || !is_sha256(&record.evidence_hash)
                    || !record.failure_code.is_empty()
                {
                    return Err(AppError::blocked("subagent completed binding 오류"));
                }
            } else if record.failure_code.is_empty() || has_result_binding(record) {
                return Err(AppError::blocked(
                    "subagent terminal failure/result binding 오류",
                ));
            }
        }
        _ => unreachable!(),
    }
    if installed
        && (record.revision == 0
            || record.revision > MAX_RECORD_REVISIONS
            || (record.previous_hash != "none" && !is_sha256(&record.previous_hash))
            || !is_sha256(&record.artifact_hash)
            || integrity::sha256_text(&render_payload(record)) != record.artifact_hash)
    {
        return Err(AppError::blocked("subagent canonical hash binding 오류"));
    }
    Ok(())
}

pub(crate) fn immutable_binding_changed(
    current: &SubagentRecordV1,
    next: &SubagentRecordV1,
) -> bool {
    current.subagent_id != next.subagent_id
        || current.project_id != next.project_id
        || current.session_id != next.session_id
        || current.parent_workflow_id != next.parent_workflow_id
        || current.parent_revision != next.parent_revision
        || current.parent_artifact_hash != next.parent_artifact_hash
        || current.role != next.role
        || current.task_hash != next.task_hash
        || current.declared_tools != next.declared_tools
        || current.read_paths != next.read_paths
        || current.write_paths != next.write_paths
        || current.timeout_ms != next.timeout_ms
        || current.requested_max_tokens != next.requested_max_tokens
        || current.created_at_ms != next.created_at_ms
}

pub(crate) fn validate_subagent_id(value: &str) -> Result<(), AppError> {
    if !value.starts_with("subagent-")
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(AppError::blocked(format!("subagent id 형식 오류: {value}")));
    }
    Ok(())
}

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn has_result_binding(record: &SubagentRecordV1) -> bool {
    !record.result_artifact_id.is_empty()
        || !record.result_artifact_hash.is_empty()
        || !record.evidence_id.is_empty()
        || !record.evidence_hash.is_empty()
}

fn validate_stored_tools_and_paths(record: &SubagentRecordV1) -> Result<(), AppError> {
    let tool_inputs = record.declared_tools.clone();
    if normalize_tools(record.role, &tool_inputs)? != record.declared_tools {
        return Err(AppError::blocked("subagent canonical tool order 오류"));
    }
    let read_inputs = record.read_paths.clone();
    if normalize_paths("read", &read_inputs, true)? != record.read_paths {
        return Err(AppError::blocked("subagent canonical read path order 오류"));
    }
    let write_inputs = record.write_paths.clone();
    if normalize_paths("write", &write_inputs, false)? != record.write_paths {
        return Err(AppError::blocked(
            "subagent canonical write path order 오류",
        ));
    }
    let has_render_diff = record
        .declared_tools
        .iter()
        .any(|tool| tool == "render_diff");
    if (record.role != SubagentRole::Executor && !record.write_paths.is_empty())
        || has_render_diff != !record.write_paths.is_empty()
    {
        return Err(AppError::blocked("subagent tool/write binding 오류"));
    }
    Ok(())
}
