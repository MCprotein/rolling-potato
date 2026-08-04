use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::runtime_core::agent::{AgentToolId, ToolObservation};

use super::{denied, observation, tool_error};
use crate::runtime_core::agent::{ToolObservationReason, ToolObservationStatus};

#[derive(Clone, Copy)]
pub(super) enum EntryKind {
    Any,
    RegularFile,
    Directory,
}

pub(super) fn resolve_existing(
    root: &Path,
    raw: &str,
    kind: EntryKind,
) -> Result<PathBuf, PathFailure> {
    validate_relative_path(raw)?;
    reject_symlink_components(root, Path::new(raw), false)?;
    resolve_existing_path(root, Path::new(raw), kind)
}

pub(super) fn resolve_existing_path(
    root: &Path,
    relative: &Path,
    kind: EntryKind,
) -> Result<PathBuf, PathFailure> {
    reject_symlink_components(root, relative, false)?;
    let candidate = root.join(relative);
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            PathFailure::NotFound
        } else {
            PathFailure::ToolError(format!("path canonicalize failed: {error}"))
        }
    })?;
    if !canonical.starts_with(root) {
        return Err(PathFailure::Denied("path escapes the project root"));
    }
    let metadata = fs::metadata(&canonical)
        .map_err(|error| PathFailure::ToolError(format!("path metadata failed: {error}")))?;
    let valid_kind = match kind {
        EntryKind::Any => metadata.is_file() || metadata.is_dir(),
        EntryKind::RegularFile => metadata.is_file(),
        EntryKind::Directory => metadata.is_dir(),
    };
    if !valid_kind {
        return Err(PathFailure::Denied("path has an unsupported file type"));
    }
    Ok(canonical)
}

pub(super) fn validate_scoped_operand(root: &Path, raw: &str) -> Result<(), PathFailure> {
    validate_relative_path(raw)?;
    reject_symlink_components(root, Path::new(raw), true)
}

fn reject_symlink_components(
    root: &Path,
    relative: &Path,
    allow_missing_tail: bool,
) -> Result<(), PathFailure> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        if matches!(component, Component::CurDir) {
            continue;
        }
        let Component::Normal(component) = component else {
            return Err(PathFailure::Denied("path must stay project-relative"));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(PathFailure::Denied("symlinks are not allowed"));
            }
            Ok(_) => {}
            Err(error) if allow_missing_tail && error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(PathFailure::NotFound);
            }
            Err(error) => {
                return Err(PathFailure::ToolError(format!(
                    "path metadata failed: {error}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_relative_path(raw: &str) -> Result<(), PathFailure> {
    if raw.is_empty() || raw.contains('\0') || raw.contains('\\') {
        return Err(PathFailure::Denied("path is empty or invalid"));
    }
    let path = Path::new(raw);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PathFailure::Denied("path must stay project-relative"));
    }
    Ok(())
}

pub(super) enum PathFailure {
    NotFound,
    Denied(&'static str),
    ToolError(String),
}

impl PathFailure {
    pub(super) fn into_observation(self, tool: AgentToolId) -> ToolObservation {
        match self {
            Self::NotFound => observation(
                tool,
                ToolObservationStatus::NotFound,
                ToolObservationReason::NotFound,
                "path not found",
            ),
            Self::Denied(message) => denied(tool, message),
            Self::ToolError(message) => tool_error(tool, message),
        }
    }
}
