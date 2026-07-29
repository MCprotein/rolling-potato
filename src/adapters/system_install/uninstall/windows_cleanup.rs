use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::super::InstallPaths;
use crate::foundation::error::AppError;

pub(super) fn schedule_windows_self_delete(paths: &InstallPaths) -> Result<(), AppError> {
    let script_path = env::temp_dir().join(format!(
        "rpotato-clean-uninstall-{}-{}.ps1",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    let script = r#"param(
    [Parameter(Mandatory=$true)][int]$ParentPid,
    [Parameter(Mandatory=$true)][string]$Target,
    [Parameter(Mandatory=$true)][string]$BinDir,
    [Parameter(Mandatory=$true)][string]$ScriptPath
)
for ($attempt = 0; $attempt -lt 300; $attempt++) {
    if (-not (Get-Process -Id $ParentPid -ErrorAction SilentlyContinue)) { break }
    Start-Sleep -Milliseconds 100
}
if (Get-Process -Id $ParentPid -ErrorAction SilentlyContinue) { exit 1 }
Remove-Item -LiteralPath $Target -Force -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $Target) { exit 1 }
if (Test-Path -LiteralPath $BinDir) {
    $remaining = @(Get-ChildItem -LiteralPath $BinDir -Force -ErrorAction SilentlyContinue)
    if ($remaining.Count -eq 0) {
        Remove-Item -LiteralPath $BinDir -Force -ErrorAction SilentlyContinue
    }
}
$installRoot = Split-Path -Parent $BinDir
if (Test-Path -LiteralPath $installRoot) {
    $remaining = @(Get-ChildItem -LiteralPath $installRoot -Force -ErrorAction SilentlyContinue)
    if ($remaining.Count -eq 0) {
        Remove-Item -LiteralPath $installRoot -Force -ErrorAction SilentlyContinue
    }
}
Remove-Item -LiteralPath $ScriptPath -Force -ErrorAction SilentlyContinue
"#;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(&script_path).map_err(|err| {
        AppError::runtime(format!(
            "Windows clean uninstall cleanup script 생성 실패: {} ({err})",
            script_path.display()
        ))
    })?;
    file.write_all(script.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|err| {
            let _ = fs::remove_file(&script_path);
            AppError::runtime(format!(
                "Windows clean uninstall cleanup script 기록 실패: {err}"
            ))
        })?;
    drop(file);

    let spawned = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&script_path)
        .arg(std::process::id().to_string())
        .arg(&paths.installed_binary)
        .arg(&paths.user_bin)
        .arg(&script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(err) = spawned {
        let _ = fs::remove_file(&script_path);
        return Err(AppError::runtime(format!(
            "Windows clean uninstall post-exit cleanup 시작 실패: {err}"
        )));
    }
    Ok(())
}

pub(super) fn remove_empty_windows_install_dirs(bin_dir: &Path) -> Result<(), AppError> {
    for path in [Some(bin_dir), bin_dir.parent()].into_iter().flatten() {
        let mut entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(AppError::runtime(format!(
                    "Windows CLI directory 확인 실패: {} ({err})",
                    path.display()
                )));
            }
        };
        if entries.next().is_some() {
            continue;
        }
        fs::remove_dir(path).map_err(|err| {
            AppError::runtime(format!(
                "Windows CLI directory 정리 실패: {} ({err})",
                path.display()
            ))
        })?;
    }
    Ok(())
}
