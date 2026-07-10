use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessLiveness {
    Alive,
    Dead,
    Unknown,
}

pub struct RecoverableLease {
    path: PathBuf,
    nonce: String,
}

impl RecoverableLease {
    pub fn acquire(path: PathBuf, context: &str) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AppError::runtime(format!("{context} lock directory 실패: {err}"))
            })?;
        }
        let nonce = format!("{}-{}", std::process::id(), now_nanos());
        let body = format!("pid={}\nnonce={}\n", std::process::id(), nonce);
        match create_lease(&path, &body) {
            Ok(()) => return Ok(Self { path, nonce }),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => {
                return Err(AppError::runtime(format!(
                    "{context} lock 생성 실패: {err}"
                )))
            }
        }

        let existing = fs::read_to_string(&path)
            .map_err(|err| AppError::blocked(format!("{context} lock 읽기 차단: {err}")))?;
        let (owner_pid, _) = parse_lease(&existing, context)?;
        match process_liveness(owner_pid) {
            ProcessLiveness::Alive | ProcessLiveness::Unknown => {
                return Err(AppError::blocked(format!(
                    "{context} lock 차단\n- owner pid: {owner_pid}\n- liveness: {:?}",
                    process_liveness(owner_pid)
                )))
            }
            ProcessLiveness::Dead => {}
        }
        let reclaimed = path.with_extension(format!("stale.{}.{}", owner_pid, now_nanos()));
        fs::rename(&path, &reclaimed).map_err(|err| {
            AppError::blocked(format!(
                "{context} dead-owner lock atomic reclaim 실패: {err}"
            ))
        })?;
        match create_lease(&path, &body) {
            Ok(()) => {
                let _ = fs::remove_file(reclaimed);
                Ok(Self { path, nonce })
            }
            Err(err) => Err(AppError::blocked(format!(
                "{context} lock reclaim 경쟁 차단: {err}"
            ))),
        }
    }
}

impl Drop for RecoverableLease {
    fn drop(&mut self) {
        let Ok(body) = fs::read_to_string(&self.path) else {
            return;
        };
        if parse_lease(&body, "lease cleanup")
            .map(|(_, nonce)| nonce == self.nonce)
            .unwrap_or(false)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn create_lease(path: &Path, body: &str) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(body.as_bytes())?;
    file.sync_all()
}

fn parse_lease(body: &str, context: &str) -> Result<(u32, String), AppError> {
    let mut pid = None;
    let mut nonce = None;
    for line in body.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| AppError::blocked(format!("{context} malformed lease")))?;
        match key {
            "pid" if pid.is_none() => pid = value.parse().ok(),
            "nonce" if nonce.is_none() && !value.is_empty() => nonce = Some(value.to_string()),
            _ => return Err(AppError::blocked(format!("{context} malformed lease"))),
        }
    }
    Ok((
        pid.ok_or_else(|| AppError::blocked(format!("{context} lease pid 누락")))?,
        nonce.ok_or_else(|| AppError::blocked(format!("{context} lease nonce 누락")))?,
    ))
}

#[cfg(unix)]
pub fn process_liveness(pid: u32) -> ProcessLiveness {
    if pid == 0 || pid > i32::MAX as u32 {
        return ProcessLiveness::Dead;
    }
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let result = unsafe { kill(pid as i32, 0) };
    if result == 0 {
        ProcessLiveness::Alive
    } else {
        match std::io::Error::last_os_error().raw_os_error() {
            Some(3) => ProcessLiveness::Dead,
            Some(1) => ProcessLiveness::Alive,
            _ => ProcessLiveness::Unknown,
        }
    }
}

#[cfg(windows)]
pub fn process_liveness(pid: u32) -> ProcessLiveness {
    type Handle = *mut std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
        fn CloseHandle(object: Handle) -> i32;
    }
    const QUERY: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    let handle = unsafe { OpenProcess(QUERY, 0, pid) };
    if handle.is_null() {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(87) => ProcessLiveness::Dead,
            Some(5) => ProcessLiveness::Unknown,
            _ => ProcessLiveness::Unknown,
        };
    }
    let mut code = 0;
    let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        ProcessLiveness::Unknown
    } else if code == STILL_ACTIVE {
        ProcessLiveness::Alive
    } else {
        ProcessLiveness::Dead
    }
}

#[cfg(not(any(unix, windows)))]
pub fn process_liveness(_pid: u32) -> ProcessLiveness {
    ProcessLiveness::Unknown
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_owner_is_excluded_and_dead_owner_is_reclaimed() {
        let root = std::env::temp_dir().join(format!("rpotato-lease-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("lease.lock");
        let first = RecoverableLease::acquire(path.clone(), "test").unwrap();
        assert!(RecoverableLease::acquire(path.clone(), "test").is_err());
        drop(first);
        fs::write(&path, "pid=4294967295\nnonce=dead\n").unwrap();
        let reclaimed = RecoverableLease::acquire(path.clone(), "test").unwrap();
        drop(reclaimed);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn abruptly_killed_owner_lease_is_reclaimed_but_live_owner_is_excluded() {
        use std::process::Command;

        let root = std::env::temp_dir().join(format!("rpotato-lease-kill-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("lease.lock");
        let mut owner = Command::new("sleep").arg("30").spawn().unwrap();
        fs::write(&path, format!("pid={}\nnonce=child\n", owner.id())).unwrap();
        assert!(RecoverableLease::acquire(path.clone(), "test").is_err());
        owner.kill().unwrap();
        owner.wait().unwrap();
        let reclaimed = RecoverableLease::acquire(path, "test").unwrap();
        drop(reclaimed);
        let _ = fs::remove_dir_all(root);
    }
}
