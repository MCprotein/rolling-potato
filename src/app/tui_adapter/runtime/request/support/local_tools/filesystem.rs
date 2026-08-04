use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::runtime_core::agent::{
    AgentToolId, ToolObservation, ToolObservationReason, ToolObservationStatus,
};
use crate::runtime_core::inference::cancellation::RequestCancellationToken;

use super::path::{resolve_existing, EntryKind};
use super::{bounded_from_parts, denied, observation, tool_error, MAX_OUTPUT_BYTES};

const MAX_READ_LINES: usize = 400;
const MAX_DIRECTORY_ENTRIES: usize = 256;
const MAX_DIRECTORY_SCAN_ENTRIES: usize = 1024;

pub(super) fn read_file(
    root: &Path,
    input: &str,
    cancellation: &RequestCancellationToken,
    timeout: Duration,
) -> ToolObservation {
    let path = match resolve_existing(root, input, EntryKind::RegularFile) {
        Ok(path) => path,
        Err(failure) => return failure.into_observation(AgentToolId::ReadFile),
    };
    let original_bytes = fs::metadata(&path)
        .ok()
        .and_then(|metadata| usize::try_from(metadata.len()).ok())
        .unwrap_or(MAX_OUTPUT_BYTES + 1);
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(error) => return tool_error(AgentToolId::ReadFile, format!("read failed: {error}")),
    };
    let started = Instant::now();
    let mut bytes = Vec::with_capacity(MAX_OUTPUT_BYTES + 1);
    let mut buffer = [0u8; 4096];
    while bytes.len() <= MAX_OUTPUT_BYTES && count_lines(&bytes) <= MAX_READ_LINES {
        if let Some(terminal) =
            terminal_observation(AgentToolId::ReadFile, cancellation, started, timeout)
        {
            return terminal;
        }
        let count = match file.read(&mut buffer) {
            Ok(count) => count,
            Err(error) => {
                return tool_error(AgentToolId::ReadFile, format!("read failed: {error}"))
            }
        };
        if count == 0 {
            break;
        }
        let remaining = (MAX_OUTPUT_BYTES + 1).saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..count.min(remaining)]);
        if remaining == 0 {
            break;
        }
    }
    if bytes.contains(&0) {
        return denied(AgentToolId::ReadFile, "file contains NUL bytes");
    }
    let contents = match utf8_prefix(bytes) {
        Ok(contents) => contents,
        Err(()) => return denied(AgentToolId::ReadFile, "file is not valid UTF-8"),
    };
    let line_bound = byte_index_after_lines(&contents, MAX_READ_LINES);
    let limit = MAX_OUTPUT_BYTES.min(line_bound);
    let returned = super::truncate_bytes(&contents, limit).to_string();
    bounded_from_parts(
        AgentToolId::ReadFile,
        original_bytes,
        returned,
        original_bytes > limit,
    )
}

pub(super) fn list_directory(
    root: &Path,
    input: &str,
    cancellation: &RequestCancellationToken,
    timeout: Duration,
) -> ToolObservation {
    let directory = match resolve_existing(root, input, EntryKind::Directory) {
        Ok(path) => path,
        Err(failure) => return failure.into_observation(AgentToolId::ListDirectory),
    };
    let read_dir = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) => {
            return tool_error(
                AgentToolId::ListDirectory,
                format!("directory read failed: {error}"),
            )
        }
    };
    let mut entries = Vec::new();
    let started = Instant::now();
    let mut scan_truncated = false;
    let mut scanned = 0usize;
    for entry in read_dir.flatten() {
        if let Some(terminal) =
            terminal_observation(AgentToolId::ListDirectory, cancellation, started, timeout)
        {
            return terminal;
        }
        if scanned >= MAX_DIRECTORY_SCAN_ENTRIES {
            scan_truncated = true;
            break;
        }
        scanned += 1;
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let kind = if file_type.is_symlink() {
            "symlink"
        } else if file_type.is_dir() {
            "directory"
        } else if file_type.is_file() {
            "file"
        } else {
            continue;
        };
        entries.push((name, kind));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let rendered = entries
        .iter()
        .map(|(name, kind)| format!("{kind}\t{name}"))
        .collect::<Vec<_>>();
    let original = join_lines(&rendered);
    let limited = join_lines(&rendered[..rendered.len().min(MAX_DIRECTORY_ENTRIES)]);
    bounded_from_parts(
        AgentToolId::ListDirectory,
        original.len(),
        limited,
        scan_truncated || rendered.len() > MAX_DIRECTORY_ENTRIES,
    )
}

fn count_lines(bytes: &[u8]) -> usize {
    bytes.iter().filter(|byte| **byte == b'\n').count()
}

fn utf8_prefix(mut bytes: Vec<u8>) -> Result<String, ()> {
    loop {
        match String::from_utf8(bytes) {
            Ok(contents) => return Ok(contents),
            Err(error) if error.utf8_error().error_len().is_none() => {
                let valid_up_to = error.utf8_error().valid_up_to();
                bytes = error.into_bytes();
                bytes.truncate(valid_up_to);
            }
            Err(_) => return Err(()),
        }
    }
}

fn terminal_observation(
    tool: AgentToolId,
    cancellation: &RequestCancellationToken,
    started: Instant,
    timeout: Duration,
) -> Option<ToolObservation> {
    if cancellation.is_cancelled() {
        Some(observation(
            tool,
            ToolObservationStatus::Cancelled,
            ToolObservationReason::RequestCancelled,
            "request cancelled",
        ))
    } else if started.elapsed() >= timeout {
        Some(observation(
            tool,
            ToolObservationStatus::Timeout,
            ToolObservationReason::ToolTimedOut,
            "tool timed out",
        ))
    } else {
        None
    }
}

fn byte_index_after_lines(contents: &str, limit: usize) -> usize {
    let mut lines = 0;
    for (index, byte) in contents.bytes().enumerate() {
        if byte == b'\n' {
            lines += 1;
            if lines == limit {
                return index + 1;
            }
        }
    }
    contents.len()
}

fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}
