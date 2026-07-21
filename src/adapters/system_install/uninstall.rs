//! Managed CLI removal, owned PATH cleanup, and Windows post-exit self-delete.

#[cfg(windows)]
use std::env;
use std::fs;
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use super::resolve_profile_target;
use super::{
    atomic_write, exact_line_ranges, validate_clean_targets, Change, InstallPaths,
    PathRegistration, PROFILE_BEGIN, PROFILE_END,
};
use crate::foundation::error::AppError;

const INSTALL_OWNER_FILE: &str = ".rpotato-install-owned";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BinaryRemovalResult {
    pub(crate) change: Change,
    pub(crate) deferred_until_exit: bool,
}

pub(crate) fn user_path_removal_plan(paths: &InstallPaths) -> Result<PathRegistration, AppError> {
    #[cfg(unix)]
    {
        let updates = unix_path_removals(paths)?;
        return Ok(summarize_unix_path_removals(&updates));
    }
    #[cfg(windows)]
    {
        return windows_path_removal(paths, false, super::WindowsPathScope::User, 1).and_then(
            |mut registrations| {
                registrations.pop().ok_or_else(|| {
                    AppError::runtime("Windows 사용자 PATH 삭제 plan 결과가 없습니다.")
                })
            },
        );
    }
    #[allow(unreachable_code)]
    Err(AppError::blocked(
        "이 운영체제의 사용자 PATH 자동 삭제는 아직 지원하지 않습니다.",
    ))
}

pub(crate) fn remove_user_path(paths: &InstallPaths) -> Result<PathRegistration, AppError> {
    #[cfg(unix)]
    {
        let updates = unix_path_removals(paths)?;
        for update in &updates {
            if update.registration.change != Change::Unchanged {
                atomic_write::atomic_replace_bytes(
                    &update.writable_profile,
                    update.updated.as_bytes(),
                )?;
            }
        }
        return Ok(summarize_unix_path_removals(&updates));
    }
    #[cfg(windows)]
    {
        return windows_path_removal(paths, true, super::WindowsPathScope::User, 1).and_then(
            |mut registrations| {
                registrations
                    .pop()
                    .ok_or_else(|| AppError::runtime("Windows 사용자 PATH 삭제 결과가 없습니다."))
            },
        );
    }
    #[allow(unreachable_code)]
    Err(AppError::blocked(
        "이 운영체제의 사용자 PATH 자동 삭제는 아직 지원하지 않습니다.",
    ))
}

pub(crate) fn validate_clean_uninstall_targets(paths: &InstallPaths) -> Result<(), AppError> {
    validate_clean_targets(paths)?;
    if paths.installed_binary.parent() != Some(paths.user_bin.as_path()) {
        return Err(AppError::blocked(format!(
            "clean uninstall binary 경계가 유효하지 않습니다: {}",
            paths.installed_binary.display()
        )));
    }
    let expected_name = if cfg!(windows) {
        "rpotato.exe"
    } else {
        "rpotato"
    };
    if paths
        .installed_binary
        .file_name()
        .and_then(|name| name.to_str())
        != Some(expected_name)
    {
        return Err(AppError::blocked(format!(
            "clean uninstall binary 이름이 유효하지 않습니다: {}",
            paths.installed_binary.display()
        )));
    }
    Ok(())
}

pub(crate) fn binary_removal_plan(paths: &InstallPaths) -> Result<Change, AppError> {
    validate_clean_uninstall_targets(paths)?;
    if !install_is_owned(paths)? {
        return Ok(Change::Unchanged);
    }
    match fs::symlink_metadata(&paths.installed_binary) {
        Ok(metadata) if metadata.file_type().is_symlink() || metadata.is_file() => {
            Ok(Change::Removed)
        }
        Ok(_) => Err(AppError::blocked(format!(
            "clean uninstall binary target이 regular file이 아닙니다: {}",
            paths.installed_binary.display()
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Change::Unchanged),
        Err(err) => Err(AppError::runtime(format!(
            "clean uninstall binary 상태 확인 실패: {} ({err})",
            paths.installed_binary.display()
        ))),
    }
}

pub(crate) fn remove_installed_binary(
    paths: &InstallPaths,
) -> Result<BinaryRemovalResult, AppError> {
    let change = binary_removal_plan(paths)?;
    if change == Change::Unchanged {
        if install_is_owned(paths)? && !paths.installed_binary.exists() {
            remove_install_ownership(paths)?;
        }
        return Ok(BinaryRemovalResult {
            change,
            deferred_until_exit: false,
        });
    }

    #[cfg(windows)]
    if super::current_invocation_is_installed(paths) {
        schedule_windows_self_delete(paths)?;
        remove_install_ownership(paths)?;
        return Ok(BinaryRemovalResult {
            change,
            deferred_until_exit: true,
        });
    }

    fs::remove_file(&paths.installed_binary).map_err(|err| {
        AppError::runtime(format!(
            "clean uninstall binary 삭제 실패: {} ({err})",
            paths.installed_binary.display()
        ))
    })?;
    remove_install_ownership(paths)?;
    #[cfg(windows)]
    remove_empty_windows_install_dirs(&paths.user_bin)?;
    Ok(BinaryRemovalResult {
        change,
        deferred_until_exit: false,
    })
}

pub(super) fn record_install_ownership(paths: &InstallPaths) -> Result<(), AppError> {
    if install_is_owned(paths)? {
        return Ok(());
    }
    atomic_write::atomic_replace_bytes(
        &install_owner_file(paths),
        b"rpotato-owned-user-install-v1\n",
    )
}

pub(super) fn install_owner_file(paths: &InstallPaths) -> PathBuf {
    paths.user_bin.join(INSTALL_OWNER_FILE)
}

pub(super) fn install_is_owned(paths: &InstallPaths) -> Result<bool, AppError> {
    let marker = install_owner_file(paths);
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(AppError::blocked(format!(
            "설치 ownership marker 유형이 유효하지 않습니다: {}",
            marker.display()
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(AppError::runtime(format!(
            "설치 ownership marker 확인 실패: {} ({err})",
            marker.display()
        ))),
    }
}

fn remove_install_ownership(paths: &InstallPaths) -> Result<(), AppError> {
    let marker = install_owner_file(paths);
    match fs::remove_file(&marker) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::runtime(format!(
            "설치 ownership marker 삭제 실패: {} ({err})",
            marker.display()
        ))),
    }
}

#[cfg(unix)]
struct UnixPathRemoval {
    registration: PathRegistration,
    writable_profile: PathBuf,
    updated: String,
}

#[cfg(unix)]
fn unix_path_removals(paths: &InstallPaths) -> Result<Vec<UnixPathRemoval>, AppError> {
    unix_profile_candidates(&paths.user_home)
        .into_iter()
        .map(|profile| {
            let writable_profile = resolve_profile_target(&profile)?;
            let existing_bytes = if writable_profile.exists() {
                fs::read(&writable_profile).map_err(|err| {
                    AppError::runtime(format!(
                        "shell profile 읽기 실패: {} ({err})",
                        writable_profile.display()
                    ))
                })?
            } else {
                Vec::new()
            };
            let existing = String::from_utf8(existing_bytes).map_err(|_| {
                AppError::blocked(format!(
                    "shell profile이 UTF-8 text가 아니어서 자동 수정하지 않았습니다: {}",
                    writable_profile.display()
                ))
            })?;
            let updated = render_profile_without_managed_block(&existing)?;
            let change = if updated == existing {
                Change::Unchanged
            } else {
                Change::Removed
            };
            Ok(UnixPathRemoval {
                registration: PathRegistration {
                    owner: profile.display().to_string(),
                    change,
                    activation: "새 terminal을 열어 PATH 변경 적용".to_string(),
                },
                writable_profile,
                updated,
            })
        })
        .collect()
}

#[cfg(unix)]
fn summarize_unix_path_removals(updates: &[UnixPathRemoval]) -> PathRegistration {
    let removed = updates
        .iter()
        .filter(|update| update.registration.change == Change::Removed)
        .map(|update| update.registration.owner.as_str())
        .collect::<Vec<_>>();
    PathRegistration {
        owner: if removed.is_empty() {
            "supported shell profiles (no owned block)".to_string()
        } else {
            removed.join(", ")
        },
        change: if removed.is_empty() {
            Change::Unchanged
        } else {
            Change::Removed
        },
        activation: "새 terminal을 열어 PATH 변경 적용".to_string(),
    }
}

#[cfg(unix)]
fn unix_profile_candidates(home: &Path) -> [PathBuf; 5] {
    [
        home.join(".zshrc"),
        home.join(".bash_profile"),
        home.join(".bashrc"),
        home.join(".profile"),
        home.join(".config").join("fish").join("config.fish"),
    ]
}

pub(super) fn render_profile_without_managed_block(existing: &str) -> Result<String, AppError> {
    let begins = exact_line_ranges(existing, PROFILE_BEGIN);
    let ends = exact_line_ranges(existing, PROFILE_END);
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(existing.to_string()),
        ([(begin, _)], [(end, suffix_start)]) if begin < end => {
            let mut rendered = String::with_capacity(existing.len());
            rendered.push_str(&existing[..*begin]);
            rendered.push_str(&existing[*suffix_start..]);
            if rendered.trim().is_empty() {
                rendered.clear();
            }
            Ok(rendered)
        }
        _ => Err(AppError::blocked(
            "shell profile의 rpotato managed PATH marker가 손상되어 자동 수정하지 않았습니다.",
        )),
    }
}

#[cfg(windows)]
pub(super) fn windows_path_removal(
    paths: &InstallPaths,
    apply: bool,
    scope: super::WindowsPathScope,
    repetitions: u8,
) -> Result<Vec<PathRegistration>, AppError> {
    use std::process::Command;

    if scope.is_user() && !super::windows_path_is_owned(paths)? {
        return Ok((0..repetitions)
            .map(|_| PathRegistration {
                owner: scope.owner().to_string(),
                change: Change::Unchanged,
                activation: "새 PowerShell을 열어 PATH 변경 적용".to_string(),
            })
            .collect());
    }
    let target = paths.user_bin.display().to_string().replace('\'', "''");
    let mutation = if apply {
        "[Environment]::SetEnvironmentVariable('Path', $next, $scope);"
    } else {
        ""
    };
    let script = format!(
        "$target = '{target}'; \
         $scope = [EnvironmentVariableTarget]::{scope}; \
         for ($i = 0; $i -lt {repetitions}; $i++) {{ \
           $current = [Environment]::GetEnvironmentVariable('Path', $scope); \
           $parts = @($current -split ';' | Where-Object {{ $_ -ne '' }}); \
           $kept = @($parts | Where-Object {{ $_.TrimEnd('\\\\') -ine $target.TrimEnd('\\\\') }}); \
           if ($kept.Count -eq $parts.Count) {{ Write-Output 'unchanged' }} else {{ \
             $next = [String]::Join(';', $kept); \
             {mutation} \
             Write-Output 'removed' \
           }} \
         }}",
        scope = scope.powershell_name()
    );
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|err| {
            AppError::runtime(format!(
                "Windows 사용자 PATH 삭제용 PowerShell 실행 실패: {err}"
            ))
        })?;
    if !output.status.success() {
        return Err(AppError::runtime(format!(
            "Windows 사용자 PATH 삭제 실패: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let registrations = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let change = match line.trim() {
                "removed" => Change::Removed,
                "unchanged" => Change::Unchanged,
                other => {
                    return Err(AppError::runtime(format!(
                        "Windows PATH 삭제 결과가 유효하지 않습니다: {other}"
                    )))
                }
            };
            Ok(PathRegistration {
                owner: scope.owner().to_string(),
                change,
                activation: "새 PowerShell을 열어 PATH 변경 적용".to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if apply && scope.is_user() {
        super::remove_windows_path_ownership(paths)?;
    }
    Ok(registrations)
}

#[cfg(windows)]
pub(super) fn windows_path_owner_file(paths: &InstallPaths) -> PathBuf {
    paths.user_bin.join(super::WINDOWS_PATH_OWNER_FILE)
}

#[cfg(windows)]
fn schedule_windows_self_delete(paths: &InstallPaths) -> Result<(), AppError> {
    use std::process::{Command, Stdio};

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

#[cfg(windows)]
fn remove_empty_windows_install_dirs(bin_dir: &Path) -> Result<(), AppError> {
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
