use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::foundation::error::AppError;

use super::discovery::BrowserExecutable;
use super::protocol::CdpEndpoint;

const ACTIVE_PORT_FILE: &str = "DevToolsActivePort";
const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(20);
const TERMINATION_GRACE: Duration = Duration::from_secs(2);
static PROFILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
pub(crate) struct BrowserSessionOptions {
    pub(crate) headless: bool,
    pub(crate) startup_timeout: Duration,
}

impl Default for BrowserSessionOptions {
    fn default() -> Self {
        Self {
            headless: true,
            startup_timeout: Duration::from_secs(10),
        }
    }
}

pub(crate) struct BrowserSession {
    child: Option<Child>,
    profile_dir: Option<PathBuf>,
    endpoint: CdpEndpoint,
}

impl BrowserSession {
    pub(crate) fn launch(
        executable: &BrowserExecutable,
        options: BrowserSessionOptions,
    ) -> Result<Self, AppError> {
        Self::launch_under(executable, options, &std::env::temp_dir())
    }

    fn launch_under(
        executable: &BrowserExecutable,
        options: BrowserSessionOptions,
        temp_root: &Path,
    ) -> Result<Self, AppError> {
        if options.startup_timeout.is_zero() {
            return Err(AppError::usage(
                "브라우저 startup timeout은 0보다 커야 합니다.",
            ));
        }
        let profile_dir = create_profile_dir(temp_root)?;
        let mut command = browser_command(executable, &profile_dir, options.headless);
        configure_process_group(&mut command);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = fs::remove_dir_all(&profile_dir);
                return Err(AppError::runtime(format!(
                    "격리 브라우저 process를 시작하지 못했습니다: {error}"
                )));
            }
        };
        let endpoint = match wait_for_endpoint(&mut child, &profile_dir, options.startup_timeout) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                terminate_child_tree(&mut child);
                let _ = fs::remove_dir_all(&profile_dir);
                return Err(error);
            }
        };

        Ok(Self {
            child: Some(child),
            profile_dir: Some(profile_dir),
            endpoint,
        })
    }

    pub(crate) fn endpoint(&self) -> &CdpEndpoint {
        &self.endpoint
    }

    pub(crate) fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    pub(crate) fn profile_dir(&self) -> Option<&Path> {
        self.profile_dir.as_deref()
    }

    pub(crate) fn close(mut self) {
        self.cleanup();
    }

    fn cleanup(&mut self) {
        if let Some(mut child) = self.child.take() {
            terminate_child_tree(&mut child);
        }
        if let Some(profile_dir) = self.profile_dir.take() {
            let _ = fs::remove_dir_all(profile_dir);
        }
    }
}

impl Drop for BrowserSession {
    fn drop(&mut self) {
        self.cleanup();
    }
}

fn browser_command(executable: &BrowserExecutable, profile_dir: &Path, headless: bool) -> Command {
    let mut command = Command::new(&executable.path);
    command
        .arg(format!("--user-data-dir={}", profile_dir.display()))
        .arg("--remote-debugging-port=0")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-background-networking")
        .arg("--disable-component-update")
        .arg("--disable-sync")
        .arg("--metrics-recording-only")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if headless {
        command.arg("--headless=new").arg("--disable-gpu");
    }
    command.arg("about:blank");
    command
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

fn create_profile_dir(temp_root: &Path) -> Result<PathBuf, AppError> {
    for _ in 0..32 {
        let sequence = PROFILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = temp_root.join(format!(
            "rpotato-browser-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                set_private_permissions(&path)?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(AppError::runtime(format!(
                    "격리 브라우저 profile directory를 만들지 못했습니다: {error}"
                )));
            }
        }
    }
    Err(AppError::runtime(
        "격리 브라우저 profile directory 이름을 할당하지 못했습니다.",
    ))
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        let _ = fs::remove_dir(path);
        AppError::runtime(format!(
            "격리 브라우저 profile 권한을 제한하지 못했습니다: {error}"
        ))
    })
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn wait_for_endpoint(
    child: &mut Child,
    profile_dir: &Path,
    timeout: Duration,
) -> Result<CdpEndpoint, AppError> {
    let active_port = profile_dir.join(ACTIVE_PORT_FILE);
    let deadline = Instant::now() + timeout;
    let mut last_parse_error = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(AppError::runtime(format!(
                    "격리 브라우저가 DevTools 준비 전에 종료했습니다: {status}"
                )));
            }
            Ok(None) => {}
            Err(error) => {
                return Err(AppError::runtime(format!(
                    "격리 브라우저 process 상태를 확인하지 못했습니다: {error}"
                )));
            }
        }
        if let Ok(contents) = fs::read_to_string(&active_port) {
            match CdpEndpoint::from_active_port_file(&contents) {
                Ok(endpoint) => return Ok(endpoint),
                Err(error) => last_parse_error = Some(error.message),
            }
        }
        if Instant::now() >= deadline {
            let suffix = last_parse_error
                .map(|error| format!(" 마지막 endpoint 오류: {error}"))
                .unwrap_or_default();
            return Err(AppError::runtime(format!(
                "격리 브라우저 DevTools endpoint 준비가 제한 시간을 초과했습니다.{suffix}"
            )));
        }
        thread::sleep(STARTUP_POLL_INTERVAL);
    }
}

fn terminate_child_tree(child: &mut Child) {
    if child.try_wait().ok().flatten().is_some() {
        return;
    }
    request_tree_termination(child.id(), false);
    if wait_for_exit(child, TERMINATION_GRACE) {
        return;
    }
    request_tree_termination(child.id(), true);
    let _ = child.kill();
    let _ = child.wait();
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(20));
            }
            _ => return false,
        }
    }
}

#[cfg(unix)]
fn request_tree_termination(pid: u32, force: bool) {
    let signal = if force { "-KILL" } else { "-TERM" };
    let process_group = format!("-{pid}");
    let _ = Command::new("kill")
        .arg(signal)
        .arg("--")
        .arg(process_group)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(windows)]
fn request_tree_termination(pid: u32, _force: bool) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(any(unix, windows)))]
fn request_tree_termination(_pid: u32, _force: bool) {}

#[cfg(test)]
pub(super) fn launch_test_session(
    executable: &BrowserExecutable,
    options: BrowserSessionOptions,
    temp_root: &Path,
) -> Result<BrowserSession, AppError> {
    BrowserSession::launch_under(executable, options, temp_root)
}
