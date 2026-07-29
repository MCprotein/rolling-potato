//! Managed binary installation and atomic self-update.

#[cfg(windows)]
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{
    atomic_write, current_invocation_is_installed, uninstall, BinaryUpdateResult, Change,
    InstallPaths,
};
use crate::foundation::error::AppError;

const PENDING_UPDATE_MARKER_FILE: &str = ".rpotato-update-pending";

pub(crate) fn binary_install_plan(paths: &InstallPaths) -> Result<Change, AppError> {
    if current_invocation_is_installed(paths) {
        return Ok(Change::Unchanged);
    }
    if !paths.source_binary.is_file() {
        return Err(AppError::blocked(format!(
            "설치할 rpotato binary가 regular file이 아닙니다: {}",
            paths.source_binary.display()
        )));
    }
    Ok(if paths.installed_binary.exists() {
        Change::Updated
    } else {
        Change::Created
    })
}

pub(crate) fn install_binary(paths: &InstallPaths) -> Result<Change, AppError> {
    ensure_no_pending_binary_mutation(paths)?;
    let plan = binary_install_plan(paths)?;
    if plan == Change::Unchanged {
        uninstall::record_install_ownership(paths)?;
        return Ok(Change::Unchanged);
    }

    fs::create_dir_all(&paths.user_bin).map_err(|err| {
        AppError::runtime(format!(
            "사용자 CLI directory 생성 실패: {} ({err})",
            paths.user_bin.display()
        ))
    })?;
    copy_executable_atomically(&paths.source_binary, &paths.installed_binary)?;
    uninstall::record_install_ownership(paths)?;
    Ok(plan)
}

pub(crate) fn update_installed_binary(
    paths: &InstallPaths,
    staged_binary: &Path,
) -> Result<BinaryUpdateResult, AppError> {
    ensure_no_pending_binary_mutation(paths)?;
    validate_installed_update_target(paths)?;
    apply_staged_update(paths, staged_binary)
}

pub(crate) fn ensure_no_pending_binary_mutation(paths: &InstallPaths) -> Result<(), AppError> {
    let marker = pending_update_marker_path(paths);
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Err(AppError::blocked(format!(
                "pending update가 완료될 때까지 설치 binary를 변경할 수 없습니다.\n- marker: {}\n- 다음 단계: 실행 중인 rpotato를 종료한 뒤 다시 시도하세요.",
                marker.display()
            )))
        }
        Ok(_) => Err(AppError::blocked(format!(
            "pending update marker 유형이 유효하지 않아 binary 변경을 차단했습니다: {}",
            marker.display()
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::runtime(format!(
            "pending update marker 확인 실패: {} ({err})",
            marker.display()
        ))),
    }
}

pub(super) fn pending_update_marker_path(paths: &InstallPaths) -> PathBuf {
    paths.user_bin.join(PENDING_UPDATE_MARKER_FILE)
}

#[cfg(any(windows, test))]
pub(super) fn reserve_windows_update_marker(
    paths: &InstallPaths,
    operation_id: &str,
) -> Result<PathBuf, AppError> {
    fs::create_dir_all(&paths.user_bin).map_err(|err| {
        AppError::runtime(format!(
            "Windows update marker directory 생성 실패: {} ({err})",
            paths.user_bin.display()
        ))
    })?;
    let marker = pending_update_marker_path(paths);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = match options.open(&marker) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(AppError::blocked(format!(
                "pending update가 이미 예약되어 있습니다.\n- marker: {}\n- 다음 단계: 실행 중인 rpotato를 종료한 뒤 다시 시도하세요.",
                marker.display()
            )));
        }
        Err(err) => {
            return Err(AppError::runtime(format!(
                "Windows update marker 생성 실패: {} ({err})",
                marker.display()
            )));
        }
    };
    if let Err(err) = file
        .write_all(format!("{operation_id}\n").as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&marker);
        return Err(AppError::runtime(format!(
            "Windows update marker 기록 실패: {} ({err})",
            marker.display()
        )));
    }
    Ok(marker)
}

pub(crate) fn validate_installed_update_target(paths: &InstallPaths) -> Result<(), AppError> {
    if !current_invocation_is_installed(paths) {
        return Err(AppError::blocked(format!(
            "자동 업데이트는 rpotato가 관리하는 사용자 설치본에서만 적용할 수 있습니다.\n- 현재 실행 파일: {}\n- 관리 설치 경로: {}\n- 다음 단계: `rpotato install`",
            paths.source_binary.display(),
            paths.installed_binary.display()
        )));
    }
    if !uninstall::install_is_owned(paths)? {
        return Err(AppError::blocked(
            "자동 업데이트 ownership marker가 없어 설치 파일을 교체하지 않았습니다. `rpotato install`로 관리 설치본을 복구하세요.",
        ));
    }
    let installed = fs::symlink_metadata(&paths.installed_binary).map_err(|err| {
        AppError::runtime(format!(
            "설치된 rpotato binary 확인 실패: {} ({err})",
            paths.installed_binary.display()
        ))
    })?;
    if !installed.is_file() || installed.file_type().is_symlink() {
        return Err(AppError::blocked(format!(
            "설치된 rpotato target이 regular file이 아닙니다: {}",
            paths.installed_binary.display()
        )));
    }
    Ok(())
}

pub(super) fn apply_staged_update(
    paths: &InstallPaths,
    staged_binary: &Path,
) -> Result<BinaryUpdateResult, AppError> {
    let metadata = fs::symlink_metadata(staged_binary).map_err(|err| {
        AppError::runtime(format!(
            "staged update binary 확인 실패: {} ({err})",
            staged_binary.display()
        ))
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(AppError::blocked(format!(
            "staged update binary가 regular file이 아닙니다: {}",
            staged_binary.display()
        )));
    }
    #[cfg(windows)]
    {
        schedule_windows_self_update(paths, staged_binary)?;
        return Ok(BinaryUpdateResult::DeferredUntilExit);
    }
    #[cfg(not(windows))]
    {
        copy_executable_atomically(staged_binary, &paths.installed_binary)?;
        Ok(BinaryUpdateResult::Applied)
    }
}

#[cfg(windows)]
fn schedule_windows_self_update(
    paths: &InstallPaths,
    staged_binary: &Path,
) -> Result<(), AppError> {
    use std::process::{Command, Stdio};

    let operation_id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let script_path = env::temp_dir().join(format!("rpotato-self-update-{operation_id}.ps1"));
    let staged_name = staged_binary
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::runtime("Windows staged update file name이 유효하지 않습니다."))?;
    let pending_source =
        staged_binary.with_file_name(format!("{staged_name}.pending-{operation_id}"));
    let installed_name = paths
        .installed_binary
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            AppError::runtime("Windows installed binary file name이 유효하지 않습니다.")
        })?;
    let backup_path = paths
        .installed_binary
        .with_file_name(format!("{installed_name}.update-backup-{operation_id}"));
    let expected_target_sha = crate::foundation::integrity::sha256_file(&paths.installed_binary)?;
    copy_executable_atomically(staged_binary, &pending_source)?;
    let marker_path = match reserve_windows_update_marker(paths, &operation_id) {
        Ok(marker) => marker,
        Err(error) => {
            let _ = fs::remove_file(&pending_source);
            return Err(error);
        }
    };
    let script = WINDOWS_SELF_UPDATE_SCRIPT;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(&script_path).map_err(|err| {
        let _ = fs::remove_file(&pending_source);
        let _ = fs::remove_file(&marker_path);
        AppError::runtime(format!(
            "Windows self-update script 생성 실패: {} ({err})",
            script_path.display()
        ))
    })?;
    file.write_all(script.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|err| {
            let _ = fs::remove_file(&script_path);
            let _ = fs::remove_file(&pending_source);
            let _ = fs::remove_file(&marker_path);
            AppError::runtime(format!("Windows self-update script 기록 실패: {err}"))
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
        .arg(&pending_source)
        .arg(&paths.installed_binary)
        .arg(&script_path)
        .arg(&marker_path)
        .arg(&expected_target_sha)
        .arg(&backup_path)
        .arg(&operation_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Err(err) = spawned {
        let _ = fs::remove_file(&script_path);
        let _ = fs::remove_file(&pending_source);
        let _ = fs::remove_file(&marker_path);
        return Err(AppError::runtime(format!(
            "Windows post-exit self-update 시작 실패: {err}"
        )));
    }
    Ok(())
}

#[cfg(any(windows, test))]
pub(super) const WINDOWS_SELF_UPDATE_SCRIPT: &str = r#"param(
    [Parameter(Mandatory=$true)][int]$ParentPid,
    [Parameter(Mandatory=$true)][string]$Source,
    [Parameter(Mandatory=$true)][string]$Target,
    [Parameter(Mandatory=$true)][string]$ScriptPath,
    [Parameter(Mandatory=$true)][string]$MarkerPath,
    [Parameter(Mandatory=$true)][string]$ExpectedTargetSha,
    [Parameter(Mandatory=$true)][string]$BackupPath,
    [Parameter(Mandatory=$true)][string]$OperationId,
    [switch]$SkipParentWait
)
$ErrorActionPreference = 'Stop'
if (-not $SkipParentWait) {
    while (Get-Process -Id $ParentPid -ErrorAction SilentlyContinue) {
        Start-Sleep -Milliseconds 100
    }
}
function Get-Sha256Hex {
    param([Parameter(Mandatory=$true)][string]$Path)
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $sha256 = [System.Security.Cryptography.SHA256]::Create()
        try {
            return ([System.BitConverter]::ToString($sha256.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
        } finally {
            $sha256.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}
$exitCode = 0
$targetMoved = $false
try {
    if (-not (Test-Path -LiteralPath $Target -PathType Leaf)) {
        throw 'installed target is missing'
    }
    $actualTargetSha = Get-Sha256Hex -Path $Target
    if ($actualTargetSha -ne $ExpectedTargetSha.ToLowerInvariant()) {
        $exitCode = 3
    } else {
        Move-Item -LiteralPath $Target -Destination $BackupPath -Force
        $targetMoved = $true
        Move-Item -LiteralPath $Source -Destination $Target -Force
        $targetMoved = $false
        Remove-Item -LiteralPath $BackupPath -Force -ErrorAction SilentlyContinue
    }
} catch {
    if ($targetMoved -and (Test-Path -LiteralPath $BackupPath)) {
        Move-Item -LiteralPath $BackupPath -Destination $Target -Force
        $targetMoved = $false
    }
    Write-Output ("self-update error: " + $_.Exception.Message)
    $exitCode = 1
} finally {
    Remove-Item -LiteralPath $Source -Force -ErrorAction SilentlyContinue
    if ((Test-Path -LiteralPath $BackupPath) -and (Test-Path -LiteralPath $Target)) {
        Remove-Item -LiteralPath $BackupPath -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $MarkerPath -PathType Leaf) {
        $markerOwner = (Get-Content -LiteralPath $MarkerPath -Raw).Trim()
        if ($markerOwner -eq $OperationId) {
            Remove-Item -LiteralPath $MarkerPath -Force -ErrorAction SilentlyContinue
        }
    }
    Remove-Item -LiteralPath $ScriptPath -Force -ErrorAction SilentlyContinue
}
exit $exitCode
"#;

fn copy_executable_atomically(source: &Path, target: &Path) -> Result<(), AppError> {
    let parent = target
        .parent()
        .ok_or_else(|| AppError::runtime("설치 binary parent path가 없습니다."))?;
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::runtime("설치 binary file name이 유효하지 않습니다."))?;
    let temporary = parent.join(format!(".{file_name}.tmp.{}", std::process::id()));
    let mut input = fs::File::open(source).map_err(|err| {
        AppError::runtime(format!(
            "설치 source binary 열기 실패: {} ({err})",
            source.display()
        ))
    })?;
    let mut output_options = OpenOptions::new();
    output_options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        output_options.mode(0o755);
    }
    let mut output = output_options.open(&temporary).map_err(|err| {
        AppError::runtime(format!(
            "설치 temporary binary 생성 실패: {} ({err})",
            temporary.display()
        ))
    })?;
    let copied = (|| -> Result<(), AppError> {
        std::io::copy(&mut input, &mut output)
            .map_err(|err| AppError::runtime(format!("설치 binary copy 실패: {err}")))?;
        output
            .flush()
            .and_then(|_| output.sync_all())
            .map_err(|err| AppError::runtime(format!("설치 binary sync 실패: {err}")))?;
        drop(output);
        atomic_write::replace_file(&temporary, target).map_err(|err| {
            AppError::runtime(format!(
                "설치 binary 교체 실패: {} ({err})",
                target.display()
            ))
        })?;
        atomic_write::sync_parent(target)
    })();
    if copied.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    copied
}
