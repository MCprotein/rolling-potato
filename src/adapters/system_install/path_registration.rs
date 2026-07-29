//! Shell and Windows user PATH registration.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use super::uninstall;
use super::{atomic_write, Change, InstallPaths, PathRegistration, PROFILE_BEGIN, PROFILE_END};
use crate::foundation::error::AppError;

pub(crate) fn ensure_user_path(paths: &InstallPaths) -> Result<PathRegistration, AppError> {
    #[cfg(unix)]
    {
        return ensure_unix_user_path(paths);
    }
    #[cfg(windows)]
    {
        return ensure_windows_user_path(paths);
    }
    #[allow(unreachable_code)]
    Err(AppError::blocked(
        "이 운영체제의 사용자 PATH 자동 등록은 아직 지원하지 않습니다.",
    ))
}

pub(crate) fn user_path_change_plan(paths: &InstallPaths) -> Result<PathRegistration, AppError> {
    #[cfg(unix)]
    {
        return unix_path_update(paths).map(|update| update.registration);
    }
    #[cfg(windows)]
    {
        return windows_path_registration(paths, false, WindowsPathScope::User, 1).and_then(
            |mut registrations| {
                registrations
                    .pop()
                    .ok_or_else(|| AppError::runtime("Windows 사용자 PATH plan 결과가 없습니다."))
            },
        );
    }
    #[allow(unreachable_code)]
    Err(AppError::blocked(
        "이 운영체제의 사용자 PATH 자동 등록은 아직 지원하지 않습니다.",
    ))
}

#[cfg(unix)]
fn ensure_unix_user_path(paths: &InstallPaths) -> Result<PathRegistration, AppError> {
    let update = unix_path_update(paths)?;
    if update.registration.change != Change::Unchanged {
        atomic_write::atomic_replace_bytes(&update.writable_profile, update.updated.as_bytes())?;
    }
    Ok(update.registration)
}

#[cfg(unix)]
struct UnixPathUpdate {
    registration: PathRegistration,
    writable_profile: PathBuf,
    updated: String,
}

#[cfg(unix)]
fn unix_path_update(paths: &InstallPaths) -> Result<UnixPathUpdate, AppError> {
    let (profile, command) = unix_path_plan(paths);
    let block = format!("{PROFILE_BEGIN}\n{command}\n{PROFILE_END}");
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
    let updated = render_managed_profile(&existing, &block)?;
    let change = if updated == existing {
        Change::Unchanged
    } else if existing.is_empty() {
        Change::Created
    } else {
        Change::Updated
    };

    Ok(UnixPathUpdate {
        registration: PathRegistration {
            owner: profile.display().to_string(),
            change,
            activation: command,
        },
        writable_profile,
        updated,
    })
}

#[cfg(unix)]
pub(super) fn unix_path_plan(paths: &InstallPaths) -> (PathBuf, String) {
    let shell = env::var_os("SHELL")
        .and_then(|value| PathBuf::from(value).file_name().map(|name| name.to_owned()))
        .and_then(|name| name.to_str().map(str::to_string))
        .unwrap_or_else(|| "sh".to_string());
    unix_profile_and_command(&paths.user_home, &paths.user_bin, &shell)
}

#[cfg(unix)]
fn unix_profile_and_command(home: &Path, user_bin: &Path, shell: &str) -> (PathBuf, String) {
    let quoted_bin = quote_posix(user_bin);
    if shell == "fish" {
        return (
            home.join(".config").join("fish").join("config.fish"),
            format!("fish_add_path --prepend {quoted_bin}"),
        );
    }
    let profile = match shell {
        "zsh" => home.join(".zshrc"),
        "bash" if cfg!(target_os = "macos") => home.join(".bash_profile"),
        "bash" => home.join(".bashrc"),
        _ => home.join(".profile"),
    };
    (profile, format!("export PATH={quoted_bin}:\"$PATH\""))
}

#[cfg(unix)]
fn quote_posix(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\"'\"'"))
}

#[cfg(unix)]
pub(super) fn resolve_profile_target(profile: &Path) -> Result<PathBuf, AppError> {
    match fs::symlink_metadata(profile) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::canonicalize(profile).map_err(|err| {
                AppError::blocked(format!(
                "shell profile symlink target을 확인하지 못해 자동 수정하지 않았습니다: {} ({err})",
                profile.display()
            ))
            })
        }
        Ok(_) => Ok(profile.to_path_buf()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(profile.to_path_buf()),
        Err(err) => Err(AppError::runtime(format!(
            "shell profile 상태 확인 실패: {} ({err})",
            profile.display()
        ))),
    }
}

pub(super) fn render_managed_profile(existing: &str, block: &str) -> Result<String, AppError> {
    let begins = exact_line_ranges(existing, PROFILE_BEGIN);
    let ends = exact_line_ranges(existing, PROFILE_END);
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => {
            let mut rendered = existing.to_string();
            if !rendered.is_empty() {
                if !rendered.ends_with('\n') {
                    rendered.push('\n');
                }
                rendered.push('\n');
            }
            rendered.push_str(block);
            rendered.push('\n');
            Ok(rendered)
        }
        ([(begin, _)], [(end, suffix_start)]) if begin < end => {
            let mut rendered = String::new();
            rendered.push_str(&existing[..*begin]);
            rendered.push_str(block);
            rendered.push('\n');
            rendered.push_str(&existing[*suffix_start..]);
            Ok(rendered)
        }
        _ => Err(AppError::blocked(
            "shell profile의 rpotato managed PATH marker가 손상되어 자동 수정하지 않았습니다.",
        )),
    }
}

pub(super) fn exact_line_ranges(text: &str, marker: &str) -> Vec<(usize, usize)> {
    let mut offset = 0;
    text.split_inclusive('\n')
        .filter_map(|line| {
            let start = offset;
            offset += line.len();
            let without_newline = line.strip_suffix('\n').unwrap_or(line);
            let content = without_newline
                .strip_suffix('\r')
                .unwrap_or(without_newline);
            (content == marker).then_some((start, offset))
        })
        .collect()
}

#[cfg(windows)]
fn ensure_windows_user_path(paths: &InstallPaths) -> Result<PathRegistration, AppError> {
    let registration = windows_path_registration(paths, true, WindowsPathScope::User, 1).and_then(
        |mut registrations| {
            registrations
                .pop()
                .ok_or_else(|| AppError::runtime("Windows 사용자 PATH 등록 결과가 없습니다."))
        },
    )?;
    if registration.change != Change::Unchanged {
        record_windows_path_ownership(paths)?;
    }
    Ok(registration)
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
pub(super) enum WindowsPathScope {
    User,
    #[cfg(test)]
    Process,
}

#[cfg(windows)]
impl WindowsPathScope {
    pub(super) fn is_user(self) -> bool {
        match self {
            Self::User => true,
            #[cfg(test)]
            Self::Process => false,
        }
    }

    pub(super) fn powershell_name(self) -> &'static str {
        match self {
            Self::User => "User",
            #[cfg(test)]
            Self::Process => "Process",
        }
    }

    pub(super) fn owner(self) -> &'static str {
        match self {
            Self::User => "HKCU\\Environment\\Path",
            #[cfg(test)]
            Self::Process => "PowerShell process PATH",
        }
    }
}

#[cfg(windows)]
pub(super) fn windows_path_registration(
    paths: &InstallPaths,
    apply: bool,
    scope: WindowsPathScope,
    repetitions: u8,
) -> Result<Vec<PathRegistration>, AppError> {
    use std::process::Command;

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
           $found = @($parts | Where-Object {{ $_.TrimEnd('\\\\') -ieq $target.TrimEnd('\\\\') }}).Count -gt 0; \
           if ($found) {{ Write-Output 'unchanged' }} else {{ \
             $empty = [String]::IsNullOrWhiteSpace($current); \
             $next = if ($empty) {{ $target }} else {{ \"$target;$current\" }}; \
             {mutation} \
             if ($empty) {{ Write-Output 'created' }} else {{ Write-Output 'updated' }} \
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
                "Windows 사용자 PATH 등록용 PowerShell 실행 실패: {err}"
            ))
        })?;
    if !output.status.success() {
        return Err(AppError::runtime(format!(
            "Windows 사용자 PATH 등록 실패: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let escaped_activation = paths.user_bin.display().to_string().replace('\'', "''");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let change = match line.trim() {
                "created" => Change::Created,
                "updated" => Change::Updated,
                "unchanged" => Change::Unchanged,
                other => {
                    return Err(AppError::runtime(format!(
                        "Windows PATH 등록 결과가 유효하지 않습니다: {other}"
                    )))
                }
            };
            Ok(PathRegistration {
                owner: scope.owner().to_string(),
                change,
                activation: format!("$env:Path = '{escaped_activation};' + $env:Path"),
            })
        })
        .collect()
}

#[cfg(windows)]
pub(super) fn windows_path_is_owned(paths: &InstallPaths) -> Result<bool, AppError> {
    let marker = uninstall::windows_path_owner_file(paths);
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(AppError::blocked(format!(
            "Windows PATH ownership marker 유형이 유효하지 않습니다: {}",
            marker.display()
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(AppError::runtime(format!(
            "Windows PATH ownership marker 확인 실패: {} ({err})",
            marker.display()
        ))),
    }
}

#[cfg(windows)]
fn record_windows_path_ownership(paths: &InstallPaths) -> Result<(), AppError> {
    let marker = uninstall::windows_path_owner_file(paths);
    if windows_path_is_owned(paths)? {
        return Ok(());
    }
    atomic_write::atomic_replace_bytes(&marker, b"rpotato-owned-user-path-v1\n")
}

#[cfg(windows)]
pub(super) fn remove_windows_path_ownership(paths: &InstallPaths) -> Result<(), AppError> {
    let marker = uninstall::windows_path_owner_file(paths);
    match fs::remove_file(&marker) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::runtime(format!(
            "Windows PATH ownership marker 삭제 실패: {} ({err})",
            marker.display()
        ))),
    }
}
