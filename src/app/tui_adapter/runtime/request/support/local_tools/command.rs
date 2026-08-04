use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime_core::agent::{
    AgentToolId, ToolObservation, ToolObservationReason, ToolObservationStatus,
};
use crate::runtime_core::inference::cancellation::RequestCancellationToken;
use crate::runtime_core::policy::decision::parse_local_read_only_command;

use super::path::{resolve_existing, validate_scoped_operand, EntryKind, PathFailure};
use super::{bounded_observation, malformed, observation, tool_error, MAX_OUTPUT_BYTES};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);
const ALLOWED_EXECUTABLES: [&str; 7] = ["pwd", "ls", "git", "rg", "head", "tail", "wc"];

pub(super) enum ProjectGitLayout {
    Repository {
        git_dir: PathBuf,
        work_tree: PathBuf,
    },
    NotRepository,
    Unsafe,
}

pub(super) fn project_git_layout(root: &Path) -> ProjectGitLayout {
    let dot_git = root.join(".git");
    let metadata = match std::fs::symlink_metadata(&dot_git) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ProjectGitLayout::NotRepository;
        }
        Err(_) => return ProjectGitLayout::Unsafe,
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return ProjectGitLayout::Unsafe;
    }
    let Ok(git_dir) = std::fs::canonicalize(dot_git) else {
        return ProjectGitLayout::Unsafe;
    };
    if !git_dir.starts_with(root) {
        return ProjectGitLayout::Unsafe;
    }
    ProjectGitLayout::Repository {
        git_dir,
        work_tree: root.to_path_buf(),
    }
}

pub(super) struct CommandPaths {
    resolved: HashMap<&'static str, PathBuf>,
    search_path: std::ffi::OsString,
}

impl CommandPaths {
    pub(super) fn resolve(root: &Path) -> Self {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let directories = safe_path_directories(root, &path);
        let resolved = ALLOWED_EXECUTABLES
            .into_iter()
            .filter_map(|name| {
                resolve_in_directories(root, name, &directories).map(|path| (name, path))
            })
            .collect();
        let search_path = std::env::join_paths(&directories).unwrap_or_default();
        Self {
            resolved,
            search_path,
        }
    }

    pub(super) fn path_for(&self, name: &str) -> Option<&Path> {
        self.resolved.get(name).map(PathBuf::as_path)
    }
}

fn safe_path_directories(root: &Path, search_path: &std::ffi::OsStr) -> Vec<PathBuf> {
    std::env::split_paths(search_path)
        .filter(|directory| directory.is_absolute())
        .filter_map(|directory| std::fs::canonicalize(directory).ok())
        .filter(|directory| directory.is_dir() && !directory.starts_with(root))
        .fold(Vec::new(), |mut unique, directory| {
            if !unique.contains(&directory) {
                unique.push(directory);
            }
            unique
        })
}

fn resolve_in_directories(root: &Path, name: &str, directories: &[PathBuf]) -> Option<PathBuf> {
    directories
        .iter()
        .filter_map(|directory| std::fs::canonicalize(directory.join(name)).ok())
        .find(|candidate| candidate.is_file() && !candidate.starts_with(root))
}

pub(super) fn run_read_only_command(
    root: &Path,
    commands: &CommandPaths,
    input: &str,
    cancellation: &RequestCancellationToken,
    timeout: Duration,
) -> ToolObservation {
    let parsed = match parse_local_read_only_command(input) {
        Ok(parsed) => parsed,
        Err(error) => return malformed(AgentToolId::RunReadOnlyCommand, error.message),
    };
    let mut argv = parsed.argv;
    if let Err(error) = validate_command_paths(root, &argv) {
        return error.into_observation(AgentToolId::RunReadOnlyCommand);
    }
    let executable = argv.remove(0);
    let Some(executable_path) = commands.path_for(&executable) else {
        return tool_error(
            AgentToolId::RunReadOnlyCommand,
            format!("allowed executable is unavailable: {executable}"),
        );
    };
    if executable == "git" && argv.first().map(String::as_str) == Some("diff") {
        argv.insert(1, "--no-textconv".to_string());
        argv.insert(1, "--no-ext-diff".to_string());
    }
    if executable == "git" {
        let (git_dir, work_tree) = match project_git_layout(root) {
            ProjectGitLayout::Repository { git_dir, work_tree } => (git_dir, work_tree),
            ProjectGitLayout::NotRepository => {
                return tool_error(
                    AgentToolId::RunReadOnlyCommand,
                    "project root is not a Git repository",
                );
            }
            ProjectGitLayout::Unsafe => {
                return super::denied(AgentToolId::RunReadOnlyCommand, "unsafe project Git layout");
            }
        };
        argv.splice(
            0..0,
            [
                "-c".to_string(),
                "core.fsmonitor=false".to_string(),
                "-c".to_string(),
                format!("core.hooksPath={}", null_device()),
                format!("--git-dir={}", git_dir.display()),
                format!("--work-tree={}", work_tree.display()),
            ],
        );
    }
    let mut command = Command::new(executable_path);
    command.args(argv).current_dir(root);
    sanitize_command(&mut command, commands);
    match run_bounded(command, cancellation, timeout, MAX_OUTPUT_BYTES) {
        ProcessResult::Completed {
            success,
            mut stdout,
            stderr,
            truncated,
        } => {
            if !success {
                stdout.extend(stderr);
            }
            let content = String::from_utf8_lossy(&stdout).into_owned();
            let mut result =
                bounded_observation(AgentToolId::RunReadOnlyCommand, &content, MAX_OUTPUT_BYTES);
            if truncated {
                result.status = ToolObservationStatus::Truncated;
                result.reason = ToolObservationReason::OutputTruncated;
                result.truncation.truncated = true;
            } else if !success && result.status == ToolObservationStatus::Ok {
                result.status = ToolObservationStatus::ToolError;
                result.reason = ToolObservationReason::ExecutionFailed;
            }
            result
        }
        ProcessResult::Cancelled => observation(
            AgentToolId::RunReadOnlyCommand,
            ToolObservationStatus::Cancelled,
            ToolObservationReason::RequestCancelled,
            "request cancelled",
        ),
        ProcessResult::Timeout => observation(
            AgentToolId::RunReadOnlyCommand,
            ToolObservationStatus::Timeout,
            ToolObservationReason::ToolTimedOut,
            "command timed out",
        ),
        ProcessResult::Failed(message) => tool_error(AgentToolId::RunReadOnlyCommand, message),
    }
}

fn validate_command_paths(root: &Path, argv: &[String]) -> Result<(), PathFailure> {
    if matches!(argv, [git, diff, ..] if git == "git" && diff == "diff") {
        if let Some(separator) = argv.iter().position(|arg| arg == "--") {
            for path in &argv[separator + 1..] {
                validate_scoped_operand(root, path)?;
            }
        }
        return Ok(());
    }
    let paths: &[String] = match argv {
        [rg, files, separator, directory]
            if rg == "rg" && files == "--files" && separator == "--" =>
        {
            std::slice::from_ref(directory)
        }
        [rg, line_numbers, rest @ ..] if rg == "rg" && line_numbers == "-n" => {
            let separator = rest
                .iter()
                .position(|arg| arg == "--")
                .unwrap_or(rest.len());
            rest.get(separator.saturating_add(2)..).unwrap_or_default()
        }
        [command, _, _, separator, file]
            if matches!(command.as_str(), "head" | "tail") && separator == "--" =>
        {
            std::slice::from_ref(file)
        }
        [wc, lines, separator, files @ ..] if wc == "wc" && lines == "-l" && separator == "--" => {
            files
        }
        _ => &[],
    };
    for path in paths {
        let kind = match argv.first().map(String::as_str) {
            Some("head" | "tail" | "wc") => EntryKind::RegularFile,
            Some("rg") if argv.get(1).map(String::as_str) == Some("--files") => {
                EntryKind::Directory
            }
            _ => EntryKind::Any,
        };
        resolve_existing(root, path, kind)?;
    }
    Ok(())
}

pub(super) fn sanitize_command(command: &mut Command, commands: &CommandPaths) {
    command
        .env_clear()
        .env("PATH", &commands.search_path)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_PAGER", "cat")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null_device())
        .env("GIT_ATTR_NOSYSTEM", "1");
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
}

#[cfg(windows)]
pub(super) fn null_device() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
pub(super) fn null_device() -> &'static str {
    "/dev/null"
}

pub(super) enum ProcessResult {
    Completed {
        success: bool,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        truncated: bool,
    },
    Cancelled,
    Timeout,
    Failed(String),
}

pub(super) fn run_bounded(
    mut command: Command,
    cancellation: &RequestCancellationToken,
    timeout: Duration,
    output_limit: usize,
) -> ProcessResult {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return ProcessResult::Failed(format!("command spawn failed: {error}")),
    };
    let stdout_reader = child
        .stdout
        .take()
        .map(|stdout| thread::spawn(move || read_capped(stdout, output_limit)));
    let stderr_reader = child
        .stderr
        .take()
        .map(|stderr| thread::spawn(move || read_capped(stderr, output_limit)));
    let started = Instant::now();
    let outcome = loop {
        if cancellation.is_cancelled() {
            terminate_child_tree(&mut child);
            break ProcessResult::Cancelled;
        }
        if started.elapsed() >= timeout {
            terminate_child_tree(&mut child);
            break ProcessResult::Timeout;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let (stdout, stdout_truncated) = join_reader(stdout_reader, output_limit);
                let (stderr, stderr_truncated) = join_reader(stderr_reader, output_limit);
                break ProcessResult::Completed {
                    success: status.success(),
                    stdout,
                    stderr,
                    truncated: stdout_truncated || stderr_truncated,
                };
            }
            Ok(None) => thread::sleep(PROCESS_POLL_INTERVAL),
            Err(error) => {
                terminate_child_tree(&mut child);
                break ProcessResult::Failed(format!("command wait failed: {error}"));
            }
        }
    };
    outcome
}

fn join_reader(
    reader: Option<thread::JoinHandle<(Vec<u8>, bool)>>,
    limit: usize,
) -> (Vec<u8>, bool) {
    reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_else(|| (Vec::new(), limit == 0))
}

fn read_capped(mut reader: impl Read, limit: usize) -> (Vec<u8>, bool) {
    let mut returned = Vec::new();
    let mut truncated = false;
    let mut buffer = [0u8; 8192];
    loop {
        let Ok(count) = reader.read(&mut buffer) else {
            break;
        };
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(returned.len());
        returned.extend_from_slice(&buffer[..count.min(remaining)]);
        truncated |= count > remaining;
    }
    (returned, truncated)
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

fn terminate_child_tree(child: &mut std::process::Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    request_tree_termination(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn request_tree_termination(pid: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(windows)]
fn request_tree_termination(pid: u32) {
    let Some(system_root) = std::env::var_os("SystemRoot") else {
        return;
    };
    let _ = Command::new(PathBuf::from(system_root).join("System32/taskkill.exe"))
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(any(unix, windows)))]
fn request_tree_termination(_pid: u32) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn path_catalog_ignores_relative_and_project_owned_directories() {
        let base = std::env::temp_dir().join(format!(
            "rpotato-command-path-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("project");
        let project_bin = root.join("bin");
        let trusted_first = base.join("trusted-first");
        let trusted_second = base.join("trusted-second");
        std::fs::create_dir_all(&project_bin).unwrap();
        std::fs::create_dir_all(&trusted_first).unwrap();
        std::fs::create_dir_all(&trusted_second).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let search_path = std::env::join_paths([
            PathBuf::from("relative-bin"),
            project_bin,
            trusted_second.clone(),
            trusted_first.clone(),
            trusted_second.clone(),
        ])
        .unwrap();
        let directories = safe_path_directories(&root, &search_path);
        assert_eq!(
            directories,
            vec![
                std::fs::canonicalize(trusted_second).unwrap(),
                std::fs::canonicalize(trusted_first).unwrap(),
            ]
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn path_catalog_rejects_executable_symlink_targeting_project() {
        let base = std::env::temp_dir().join(format!(
            "rpotato-command-target-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let root = base.join("project");
        let trusted_bin = base.join("trusted-bin");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&trusted_bin).unwrap();
        std::fs::write(root.join("shadow"), "").unwrap();
        std::os::unix::fs::symlink(root.join("shadow"), trusted_bin.join("pwd")).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let trusted_bin = std::fs::canonicalize(trusted_bin).unwrap();
        assert!(resolve_in_directories(&root, "pwd", &[trusted_bin]).is_none());
        std::fs::remove_dir_all(base).unwrap();
    }
}
