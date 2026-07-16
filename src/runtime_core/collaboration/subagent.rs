//! Subagent role, tool, launch, and lifecycle policy.

use crate::foundation::error::AppError;
use crate::foundation::integrity;
pub(crate) use crate::runtime_core::inference::backend::MAX_CHAT_TIMEOUT_MS;
use std::collections::BTreeSet;
use std::path::{Component, Path};

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

pub fn validate_launch(
    role: &str,
    task: &str,
    declared_tools: &[String],
    read_paths: &[String],
    write_paths: &[String],
    timeout_ms: Option<u32>,
    max_tokens: Option<u32>,
) -> Result<ValidatedLaunch, AppError> {
    let role = SubagentRole::parse(role)
        .ok_or_else(|| AppError::usage(format!("지원하지 않는 subagent role입니다: {role}")))?;
    let task = task.trim();
    if task.is_empty() || task.len() > MAX_TASK_BYTES {
        return Err(AppError::usage(format!(
            "subagent task는 1..={MAX_TASK_BYTES} UTF-8 byte 범위여야 합니다."
        )));
    }
    if declared_tools.is_empty() {
        return Err(AppError::usage(
            "subagent launch는 최소 하나의 --tool 선언이 필요합니다.",
        ));
    }
    let tools = normalize_tools(role, declared_tools)?;
    let read_paths = normalize_paths("read", read_paths, true)?;
    let write_paths = normalize_paths("write", write_paths, false)?;
    let has_render_diff = tools.iter().any(|tool| tool == "render_diff");
    if role != SubagentRole::Executor && !write_paths.is_empty() {
        return Err(AppError::blocked(
            "executor가 아닌 subagent role은 write ownership을 선언할 수 없습니다.",
        ));
    }
    if has_render_diff != !write_paths.is_empty() {
        return Err(AppError::blocked(
            "render_diff tool과 하나 이상의 write path는 함께 선언해야 합니다.",
        ));
    }
    if write_paths.iter().any(|owner| {
        !read_paths.iter().any(|read| {
            read == owner
                || read
                    .strip_prefix(owner)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    }) {
        return Err(AppError::blocked(
            "subagent write ownership이 declared read target과 겹치지 않습니다.",
        ));
    }
    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    if timeout_ms == 0 || timeout_ms > MAX_CHAT_TIMEOUT_MS {
        return Err(AppError::usage(format!(
            "subagent timeout은 1..={} ms 범위여야 합니다.",
            MAX_CHAT_TIMEOUT_MS
        )));
    }
    let requested_max_tokens = max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    if requested_max_tokens == 0 || requested_max_tokens > MAX_MAX_TOKENS {
        return Err(AppError::usage(format!(
            "subagent max tokens는 1..={MAX_MAX_TOKENS} 범위여야 합니다."
        )));
    }
    Ok(ValidatedLaunch {
        role,
        task_hash: integrity::sha256_text(task),
        declared_tools: tools,
        read_paths,
        write_paths,
        timeout_ms,
        requested_max_tokens,
    })
}

pub(crate) fn normalize_tools(
    role: SubagentRole,
    declared_tools: &[String],
) -> Result<Vec<String>, AppError> {
    let mut seen = BTreeSet::new();
    for value in declared_tools {
        let tool = SubagentTool::parse(value.trim()).ok_or_else(|| {
            AppError::usage(format!("지원하지 않는 subagent tool입니다: {value}"))
        })?;
        if !role.allows(tool) {
            return Err(AppError::blocked(format!(
                "subagent role/tool policy 차단\n- role: {}\n- tool: {}",
                role.as_str(),
                tool.as_str()
            )));
        }
        if !seen.insert(tool) {
            return Err(AppError::usage(format!(
                "subagent tool은 중복 선언할 수 없습니다: {}",
                tool.as_str()
            )));
        }
    }
    if !seen.contains(&SubagentTool::ReadFile) {
        return Err(AppError::blocked(
            "v0.35 subagent는 read_file tool을 반드시 선언해야 합니다.",
        ));
    }
    Ok(seen
        .into_iter()
        .map(|tool| tool.as_str().to_string())
        .collect())
}

pub(crate) fn normalize_paths(
    kind: &str,
    values: &[String],
    required: bool,
) -> Result<Vec<String>, AppError> {
    if required && values.is_empty() {
        return Err(AppError::usage(format!(
            "subagent launch는 최소 하나의 --{kind} path가 필요합니다."
        )));
    }
    if values.len() > MAX_DECLARED_PATHS {
        return Err(AppError::usage(format!(
            "subagent {kind} path는 최대 {MAX_DECLARED_PATHS}개까지 허용합니다."
        )));
    }
    let mut normalized = BTreeSet::new();
    for value in values {
        let path = normalize_relative_path(value)?;
        if !normalized.insert(path.clone()) {
            return Err(AppError::usage(format!(
                "subagent {kind} path는 중복 선언할 수 없습니다: {path}"
            )));
        }
    }
    Ok(normalized.into_iter().collect())
}

pub(crate) fn normalize_relative_path(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.contains(['\\', ':']) {
        return Err(AppError::blocked(format!(
            "subagent path 정규화 차단: {value}"
        )));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(AppError::blocked(format!(
            "subagent absolute path 차단: {value}"
        )));
    }
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| AppError::blocked("subagent path는 UTF-8이어야 합니다."))?;
                if value.is_empty() {
                    return Err(AppError::blocked("subagent empty path component 차단"));
                }
                components.push(value);
            }
            _ => {
                return Err(AppError::blocked(format!(
                    "subagent path traversal 차단: {value}"
                )))
            }
        }
    }
    if components.is_empty() {
        return Err(AppError::blocked("subagent empty path 차단"));
    }
    Ok(components.join("/"))
}
