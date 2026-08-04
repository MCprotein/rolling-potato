use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::runtime_core::agent::{
    AgentToolId, ToolObservation, ToolObservationReason, ToolObservationStatus,
};
use crate::runtime_core::inference::cancellation::RequestCancellationToken;

use super::command::{null_device, run_bounded, sanitize_command, CommandPaths, ProcessResult};
use super::path::{resolve_existing_path, EntryKind};
use super::{malformed, observation, observation_with_truncation, MAX_OUTPUT_BYTES};

const MAX_SEARCH_FILES: usize = 10_000;
const MAX_SEARCH_MATCHES: usize = 64;
const MAX_SEARCH_LINE_CHARS: usize = 512;
const MAX_GIT_LIST_BYTES: usize = 8 * 1024 * 1024;
const MAX_FALLBACK_DIRECTORY_ENTRIES: usize = 2048;
const MAX_IGNORE_BYTES: u64 = 64 * 1024;
const MAX_VISIBLE_LINE_BYTES: usize = MAX_SEARCH_LINE_CHARS * 4;

pub(super) fn search_repository(
    root: &Path,
    commands: &CommandPaths,
    literal: &str,
    cancellation: &RequestCancellationToken,
    timeout: Duration,
) -> ToolObservation {
    if literal.is_empty() || literal.contains('\0') {
        return malformed(
            AgentToolId::SearchRepository,
            "search literal is empty or invalid",
        );
    }
    let started = Instant::now();
    let (files, file_limit_hit) = match git_candidate_files(root, commands, cancellation, timeout)
        .unwrap_or_else(|| fallback_candidate_files(root, cancellation, started, timeout))
    {
        CandidateResult::Files(files, limited) => (files, limited),
        CandidateResult::Cancelled => {
            return observation(
                AgentToolId::SearchRepository,
                ToolObservationStatus::Cancelled,
                ToolObservationReason::RequestCancelled,
                "request cancelled",
            )
        }
        CandidateResult::Timeout => {
            return observation(
                AgentToolId::SearchRepository,
                ToolObservationStatus::Timeout,
                ToolObservationReason::ToolTimedOut,
                "search timed out",
            )
        }
    };
    let mut output = String::new();
    let mut matches = 0usize;
    let mut truncated = file_limit_hit;
    'files: for relative in files {
        if cancellation.is_cancelled() {
            return observation(
                AgentToolId::SearchRepository,
                ToolObservationStatus::Cancelled,
                ToolObservationReason::RequestCancelled,
                "request cancelled",
            );
        }
        if started.elapsed() >= timeout {
            return observation(
                AgentToolId::SearchRepository,
                ToolObservationStatus::Timeout,
                ToolObservationReason::ToolTimedOut,
                "search timed out",
            );
        }
        let Ok(path) = resolve_existing_path(root, &relative, EntryKind::RegularFile) else {
            continue;
        };
        let Ok(file) = File::open(path) else { continue };
        match scan_file(
            BufReader::new(file),
            literal,
            &relative,
            &mut output,
            &mut matches,
            cancellation,
            started,
            timeout,
        ) {
            FileScan::Completed => {}
            FileScan::Limit => {
                truncated = true;
                break 'files;
            }
            FileScan::Cancelled => {
                return observation(
                    AgentToolId::SearchRepository,
                    ToolObservationStatus::Cancelled,
                    ToolObservationReason::RequestCancelled,
                    "request cancelled",
                )
            }
            FileScan::Timeout => {
                return observation(
                    AgentToolId::SearchRepository,
                    ToolObservationStatus::Timeout,
                    ToolObservationReason::ToolTimedOut,
                    "search timed out",
                )
            }
        }
    }
    observation_with_truncation(AgentToolId::SearchRepository, output, truncated, None)
}

enum FileScan {
    Completed,
    Limit,
    Cancelled,
    Timeout,
}

struct LineScanState {
    number: usize,
    prefix: Vec<u8>,
    overlap: Vec<u8>,
    matched: bool,
    has_nul: bool,
    has_data: bool,
}

impl LineScanState {
    fn new() -> Self {
        Self {
            number: 1,
            prefix: Vec::with_capacity(MAX_VISIBLE_LINE_BYTES),
            overlap: Vec::new(),
            matched: false,
            has_nul: false,
            has_data: false,
        }
    }

    fn push(&mut self, bytes: &[u8], literal: &[u8]) {
        self.has_data |= !bytes.is_empty();
        self.has_nul |= bytes.contains(&0);
        let remaining = MAX_VISIBLE_LINE_BYTES.saturating_sub(self.prefix.len());
        self.prefix
            .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        if !self.matched {
            let mut window = Vec::with_capacity(self.overlap.len() + bytes.len());
            window.extend_from_slice(&self.overlap);
            window.extend_from_slice(bytes);
            self.matched = window
                .windows(literal.len())
                .any(|candidate| candidate == literal);
            let overlap_len = literal.len().saturating_sub(1).min(window.len());
            self.overlap.clear();
            self.overlap
                .extend_from_slice(&window[window.len() - overlap_len..]);
        }
    }

    fn finish(&mut self, relative: &Path, output: &mut String, matches: &mut usize) -> bool {
        if self.prefix.last() == Some(&b'\r') {
            self.prefix.pop();
        }
        let mut limit_hit = false;
        if self.matched && !self.has_nul {
            if let Some(line) = visible_utf8_prefix(&self.prefix) {
                let record = format!("{}:{}:{}\n", slash_path(relative), self.number, line);
                if output.len() + record.len() > MAX_OUTPUT_BYTES {
                    limit_hit = true;
                } else {
                    output.push_str(&record);
                    *matches += 1;
                    limit_hit = *matches >= MAX_SEARCH_MATCHES;
                }
            }
        }
        self.number += 1;
        self.prefix.clear();
        self.overlap.clear();
        self.matched = false;
        self.has_nul = false;
        self.has_data = false;
        limit_hit
    }
}

fn scan_file(
    mut reader: BufReader<File>,
    literal: &str,
    relative: &Path,
    output: &mut String,
    matches: &mut usize,
    cancellation: &RequestCancellationToken,
    started: Instant,
    timeout: Duration,
) -> FileScan {
    let literal = literal.as_bytes();
    let mut line = LineScanState::new();
    loop {
        if cancellation.is_cancelled() {
            return FileScan::Cancelled;
        }
        if started.elapsed() >= timeout {
            return FileScan::Timeout;
        }
        let consumed = {
            let buffer = match reader.fill_buf() {
                Ok(buffer) => buffer,
                Err(_) => return FileScan::Completed,
            };
            if buffer.is_empty() {
                if line.has_data && line.finish(relative, output, matches) {
                    return FileScan::Limit;
                }
                return FileScan::Completed;
            }
            let mut cursor = 0;
            while cursor < buffer.len() {
                let remaining = &buffer[cursor..];
                if let Some(newline) = remaining.iter().position(|byte| *byte == b'\n') {
                    line.push(&remaining[..newline], literal);
                    if line.finish(relative, output, matches) {
                        return FileScan::Limit;
                    }
                    cursor += newline + 1;
                } else {
                    line.push(remaining, literal);
                    cursor = buffer.len();
                }
            }
            buffer.len()
        };
        reader.consume(consumed);
    }
}

fn visible_utf8_prefix(bytes: &[u8]) -> Option<&str> {
    match std::str::from_utf8(bytes) {
        Ok(value) => Some(truncate_chars(value, MAX_SEARCH_LINE_CHARS)),
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&bytes[..error.valid_up_to()])
                .ok()
                .map(|value| truncate_chars(value, MAX_SEARCH_LINE_CHARS))
        }
        Err(_) => None,
    }
}

enum CandidateResult {
    Files(Vec<PathBuf>, bool),
    Cancelled,
    Timeout,
}

fn git_candidate_files(
    root: &Path,
    commands: &CommandPaths,
    cancellation: &RequestCancellationToken,
    timeout: Duration,
) -> Option<CandidateResult> {
    let git = commands.path_for("git")?;
    let mut command = Command::new(git);
    command
        .args(["-c", "core.fsmonitor=false", "-c"])
        .arg(format!("core.hooksPath={}", null_device()))
        .args(["ls-files", "-co", "--exclude-standard", "-z"])
        .current_dir(root);
    sanitize_command(&mut command, commands);
    let (stdout, process_truncated) =
        match run_bounded(command, cancellation, timeout, MAX_GIT_LIST_BYTES) {
            ProcessResult::Completed {
                success: true,
                stdout,
                truncated,
                ..
            } => (stdout, truncated),
            ProcessResult::Completed { success: false, .. } | ProcessResult::Failed(_) => {
                return None
            }
            ProcessResult::Cancelled => return Some(CandidateResult::Cancelled),
            ProcessResult::Timeout => return Some(CandidateResult::Timeout),
        };
    let mut paths = Vec::new();
    let mut limit_hit = process_truncated;
    for raw in stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let Ok(relative) = std::str::from_utf8(raw) else {
            continue;
        };
        let path = PathBuf::from(relative);
        if hidden_path(&path) {
            continue;
        }
        if paths.len() >= MAX_SEARCH_FILES {
            limit_hit = true;
            break;
        }
        paths.push(path);
    }
    paths.sort_by(|left, right| slash_path(left).cmp(&slash_path(right)));
    Some(CandidateResult::Files(paths, limit_hit))
}

fn fallback_candidate_files(
    root: &Path,
    cancellation: &RequestCancellationToken,
    started: Instant,
    timeout: Duration,
) -> CandidateResult {
    let ignore = read_root_ignore(root);
    let mut files = Vec::new();
    let mut stack = vec![PathBuf::new()];
    let mut limit_hit = false;
    while let Some(relative_dir) = stack.pop() {
        if cancellation.is_cancelled() {
            return CandidateResult::Cancelled;
        }
        if started.elapsed() >= timeout {
            return CandidateResult::Timeout;
        }
        let Ok(read_dir) = fs::read_dir(root.join(&relative_dir)) else {
            continue;
        };
        let mut entries = Vec::with_capacity(MAX_FALLBACK_DIRECTORY_ENTRIES);
        let mut directory_limit_hit = false;
        for entry in read_dir.flatten() {
            if cancellation.is_cancelled() {
                return CandidateResult::Cancelled;
            }
            if started.elapsed() >= timeout {
                return CandidateResult::Timeout;
            }
            if entries.len() >= MAX_FALLBACK_DIRECTORY_ENTRIES {
                directory_limit_hit = true;
                break;
            }
            entries.push(entry);
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries.into_iter().rev() {
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let relative = relative_dir.join(name);
            if ignored_path(&relative, &ignore) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(relative);
            } else if file_type.is_file() {
                if files.len() >= MAX_SEARCH_FILES {
                    limit_hit = true;
                    break;
                }
                files.push(relative);
            }
        }
        limit_hit |= directory_limit_hit;
        if limit_hit {
            break;
        }
    }
    files.sort_by(|left, right| slash_path(left).cmp(&slash_path(right)));
    CandidateResult::Files(files, limit_hit)
}

fn read_root_ignore(root: &Path) -> Vec<String> {
    let mut contents = String::new();
    let Some(mut file) = File::open(root.join(".gitignore")).ok() else {
        return Vec::new();
    };
    if file
        .by_ref()
        .take(MAX_IGNORE_BYTES + 1)
        .read_to_string(&mut contents)
        .is_err()
    {
        return Vec::new();
    }
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .map(|line| line.trim_start_matches('/').to_string())
        .collect()
}

fn ignored_path(path: &Path, patterns: &[String]) -> bool {
    let rendered = slash_path(path);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    patterns.iter().any(|pattern| {
        if let Some(directory) = pattern.strip_suffix('/') {
            return rendered == directory || rendered.starts_with(&format!("{directory}/"));
        }
        if let Some(suffix) = pattern.strip_prefix('*') {
            return name.ends_with(suffix);
        }
        rendered == *pattern || (!pattern.contains('/') && name == pattern)
    })
}

fn truncate_chars(value: &str, max_chars: usize) -> &str {
    value
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| &value[..index])
        .unwrap_or(value)
}

fn slash_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn hidden_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(component, Component::Normal(value) if value.to_str().is_some_and(|value| value.starts_with('.')))
    })
}
