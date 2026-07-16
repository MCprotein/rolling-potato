use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::foundation::error::AppError;

#[cfg(unix)]
pub(crate) fn configure_child(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(not(unix))]
pub(crate) fn configure_child(_command: &mut Command) {}

pub(crate) fn is_running(pid: u32) -> bool {
    running_status(pid).unwrap_or(false)
}

#[cfg(unix)]
pub(crate) fn running_status(pid: u32) -> Result<bool, AppError> {
    let Some(pid_arg) = unix_pid_arg(pid) else {
        return Ok(false);
    };
    if is_zombie(&pid_arg) {
        return Ok(false);
    }
    Command::new("kill")
        .arg("-0")
        .arg(&pid_arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|err| AppError::runtime(format!("backend process 상태 확인 실패: {err}")))
}

#[cfg(unix)]
fn is_zombie(pid_arg: &str) -> bool {
    Command::new("ps")
        .arg("-p")
        .arg(pid_arg)
        .arg("-o")
        .arg("stat=")
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .trim_start()
                .starts_with('Z')
        })
        .unwrap_or(false)
}

#[cfg(unix)]
pub(crate) fn unix_pid_arg(pid: u32) -> Option<String> {
    if pid == 0 || pid > i32::MAX as u32 {
        None
    } else {
        Some(pid.to_string())
    }
}

#[cfg(windows)]
pub(crate) fn running_status(pid: u32) -> Result<bool, AppError> {
    let output = Command::new("tasklist")
        .arg("/FI")
        .arg(format!("PID eq {pid}"))
        .output()
        .map_err(|err| AppError::runtime(format!("backend process 상태 확인 실패: {err}")))?;
    if !output.status.success() {
        return Err(AppError::runtime(format!(
            "backend process 상태 확인 명령이 실패했습니다: pid={pid}"
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn running_status(_pid: u32) -> Result<bool, AppError> {
    Err(AppError::blocked(
        "현재 platform에서는 backend process 상태 확인을 지원하지 않습니다.",
    ))
}

#[cfg(unix)]
pub(crate) fn terminate(pid: u32, force: bool) -> Result<(), AppError> {
    let Some(pid_arg) = unix_pid_arg(pid) else {
        return Err(AppError::runtime(format!(
            "backend process 종료 명령이 실패했습니다: invalid unix pid={pid}"
        )));
    };
    let mut command = Command::new("kill");
    if force {
        command.arg("-9");
    }
    let status = command
        .arg(pid_arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| AppError::runtime(format!("backend process 종료 명령 실패: {err}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::runtime(format!(
            "backend process 종료 명령이 실패했습니다: pid={pid}"
        )))
    }
}

#[cfg(windows)]
pub(crate) fn terminate(pid: u32, force: bool) -> Result<(), AppError> {
    let mut command = Command::new("taskkill");
    command.arg("/PID").arg(pid.to_string()).arg("/T");
    if force {
        command.arg("/F");
    }
    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| AppError::runtime(format!("backend process 종료 명령 실패: {err}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::runtime(format!(
            "backend process 종료 명령이 실패했습니다: pid={pid}"
        )))
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn terminate(_pid: u32, _force: bool) -> Result<(), AppError> {
    Err(AppError::blocked(
        "현재 platform에서는 backend process stop을 지원하지 않습니다.",
    ))
}

pub(crate) fn wait_until_stopped(pid: u32, timeout: Duration) -> Result<bool, AppError> {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if !running_status(pid)? {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(!running_status(pid)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn unix_pid_argument_rejects_invalid_values() {
        assert_eq!(unix_pid_arg(0), None);
        assert_eq!(unix_pid_arg(u32::MAX), None);
        assert_eq!(unix_pid_arg(i32::MAX as u32), Some(i32::MAX.to_string()));
    }
}
