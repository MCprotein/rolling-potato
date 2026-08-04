//! Bounded project-scoped tools for the local agent loop.

mod command;
mod filesystem;
mod path;
mod search;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::filesystem::layout as paths;
use crate::foundation::error::AppError;
use crate::runtime_core::agent::{
    AgentToolId, LocalAgentToolCall, ToolObservation, ToolObservationReason, ToolObservationStatus,
    ToolObservationTruncation,
};
use crate::runtime_core::inference::cancellation::RequestCancellationToken;

pub(super) const MAX_OUTPUT_BYTES: usize = 16 * 1024;

pub(super) struct LocalToolExecutor {
    root: PathBuf,
    commands: command::CommandPaths,
}

impl LocalToolExecutor {
    pub(super) fn for_current_project() -> Result<Self, AppError> {
        canonical_project_root().map(|root| {
            let commands = command::CommandPaths::resolve(&root);
            Self { root, commands }
        })
    }

    pub(super) fn execute(
        &self,
        call: &LocalAgentToolCall,
        cancellation: &RequestCancellationToken,
        timeout: Duration,
    ) -> ToolObservation {
        if cancellation.is_cancelled() {
            return observation(
                call.id,
                ToolObservationStatus::Cancelled,
                ToolObservationReason::RequestCancelled,
                "request cancelled",
            );
        }
        match call.id {
            AgentToolId::ReadFile => {
                filesystem::read_file(&self.root, call.input.trim(), cancellation, timeout)
            }
            AgentToolId::ListDirectory => {
                filesystem::list_directory(&self.root, call.input.trim(), cancellation, timeout)
            }
            AgentToolId::SearchRepository => search::search_repository(
                &self.root,
                &self.commands,
                &call.input,
                cancellation,
                timeout,
            ),
            AgentToolId::RunReadOnlyCommand => command::run_read_only_command(
                &self.root,
                &self.commands,
                &call.input,
                cancellation,
                timeout,
            ),
            _ => denied(call.id, "tool is not a project-scoped local tool"),
        }
    }
}

fn canonical_project_root() -> Result<PathBuf, AppError> {
    let root = paths::project_root();
    fs::canonicalize(&root).map_err(|error| {
        AppError::runtime(format!(
            "project root를 canonicalize하지 못했습니다: {} ({error})",
            root.display()
        ))
    })
}

pub(super) fn truncate_bytes(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &value[..boundary]
}

pub(super) fn bounded_observation(
    tool: AgentToolId,
    content: &str,
    max_bytes: usize,
) -> ToolObservation {
    let returned = truncate_bytes(content, max_bytes).to_string();
    let truncated = returned.len() < content.len();
    observation_with_truncation(tool, returned, truncated, Some(content.len()))
}

pub(super) fn bounded_from_parts(
    tool: AgentToolId,
    original_bytes: usize,
    content: String,
    already_truncated: bool,
) -> ToolObservation {
    let returned = truncate_bytes(&content, MAX_OUTPUT_BYTES).to_string();
    observation_with_truncation(
        tool,
        returned,
        already_truncated || content.len() > MAX_OUTPUT_BYTES,
        Some(original_bytes),
    )
}

pub(super) fn observation_with_truncation(
    tool: AgentToolId,
    content: String,
    truncated: bool,
    original_bytes: Option<usize>,
) -> ToolObservation {
    let returned_bytes = content.len();
    ToolObservation {
        tool_id: Some(tool),
        status: if truncated {
            ToolObservationStatus::Truncated
        } else {
            ToolObservationStatus::Ok
        },
        reason: if truncated {
            ToolObservationReason::OutputTruncated
        } else {
            ToolObservationReason::Completed
        },
        content,
        truncation: ToolObservationTruncation {
            truncated,
            original_bytes: original_bytes.unwrap_or(returned_bytes + usize::from(truncated)),
            returned_bytes,
        },
    }
}

pub(super) fn observation(
    tool: AgentToolId,
    status: ToolObservationStatus,
    reason: ToolObservationReason,
    content: impl Into<String>,
) -> ToolObservation {
    ToolObservation::new(Some(tool), status, reason, content)
}

pub(super) fn denied(tool: AgentToolId, message: impl Into<String>) -> ToolObservation {
    observation(
        tool,
        ToolObservationStatus::Denied,
        ToolObservationReason::PolicyDenied,
        message,
    )
}

pub(super) fn malformed(tool: AgentToolId, message: impl Into<String>) -> ToolObservation {
    observation(
        tool,
        ToolObservationStatus::Malformed,
        ToolObservationReason::InvalidArguments,
        message,
    )
}

pub(super) fn tool_error(tool: AgentToolId, message: impl Into<String>) -> ToolObservation {
    observation(
        tool,
        ToolObservationStatus::ToolError,
        ToolObservationReason::ExecutionFailed,
        message,
    )
}

#[cfg(test)]
mod tests;
