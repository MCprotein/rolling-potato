use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use super::super::resolve_profile_target;
use super::super::{
    atomic_write, exact_line_ranges, Change, InstallPaths, PathRegistration, PROFILE_BEGIN,
    PROFILE_END,
};
use crate::foundation::error::AppError;

pub(crate) fn user_path_removal_plan(paths: &InstallPaths) -> Result<PathRegistration, AppError> {
    #[cfg(unix)]
    {
        let updates = unix_path_removals(paths)?;
        return Ok(summarize_unix_path_removals(&updates));
    }
    #[cfg(windows)]
    {
        return windows_path_removal(paths, false, super::super::WindowsPathScope::User, 1)
            .and_then(|mut registrations| {
                registrations.pop().ok_or_else(|| {
                    AppError::runtime("Windows 사용자 PATH 삭제 plan 결과가 없습니다.")
                })
            });
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
        return windows_path_removal(paths, true, super::super::WindowsPathScope::User, 1)
            .and_then(|mut registrations| {
                registrations
                    .pop()
                    .ok_or_else(|| AppError::runtime("Windows 사용자 PATH 삭제 결과가 없습니다."))
            });
    }
    #[allow(unreachable_code)]
    Err(AppError::blocked(
        "이 운영체제의 사용자 PATH 자동 삭제는 아직 지원하지 않습니다.",
    ))
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

pub(in crate::adapters::system_install) fn render_profile_without_managed_block(
    existing: &str,
) -> Result<String, AppError> {
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
pub(in crate::adapters::system_install) fn windows_path_removal(
    paths: &InstallPaths,
    apply: bool,
    scope: super::super::WindowsPathScope,
    repetitions: u8,
) -> Result<Vec<PathRegistration>, AppError> {
    use std::process::Command;

    if scope.is_user() && !super::super::windows_path_is_owned(paths)? {
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
        super::super::remove_windows_path_ownership(paths)?;
    }
    Ok(registrations)
}

#[cfg(windows)]
pub(in crate::adapters::system_install) fn windows_path_owner_file(
    paths: &InstallPaths,
) -> PathBuf {
    paths.user_bin.join(super::super::WINDOWS_PATH_OWNER_FILE)
}
