//! User-local CLI installation, shell PATH registration, and clean-state removal.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapters::filesystem::{atomic_write, layout};
use crate::foundation::error::AppError;

const PROFILE_BEGIN: &str = "# >>> rpotato managed PATH >>>";
const PROFILE_END: &str = "# <<< rpotato managed PATH <<<";

#[derive(Debug, Clone)]
pub(crate) struct InstallPaths {
    pub(crate) source_binary: PathBuf,
    pub(crate) installed_binary: PathBuf,
    pub(crate) user_bin: PathBuf,
    pub(crate) user_home: PathBuf,
    pub(crate) app_data: PathBuf,
    pub(crate) project_root: PathBuf,
    pub(crate) project_state: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    Created,
    Updated,
    Unchanged,
}

impl Change {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PathRegistration {
    pub(crate) owner: String,
    pub(crate) change: Change,
    pub(crate) activation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanStateResult {
    pub(crate) app_data_removed: bool,
    pub(crate) project_state_removed: bool,
}

pub(crate) fn install_paths() -> Result<InstallPaths, AppError> {
    let source_binary = env::current_exe()
        .map_err(|err| AppError::runtime(format!("현재 rpotato 실행 경로 확인 실패: {err}")))?;
    let user_home = user_home()?;
    let user_bin = user_bin_dir(&user_home)?;
    let binary_name = if cfg!(windows) {
        "rpotato.exe"
    } else {
        "rpotato"
    };
    let project_root = layout::project_root();

    Ok(InstallPaths {
        source_binary,
        installed_binary: user_bin.join(binary_name),
        user_bin,
        user_home,
        app_data: layout::app_data_root(),
        project_state: project_root.join(".rpotato"),
        project_root,
    })
}

pub(crate) fn current_invocation_is_installed(paths: &InstallPaths) -> bool {
    equivalent_path(&paths.source_binary, &paths.installed_binary)
}

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
    let plan = binary_install_plan(paths)?;
    if plan == Change::Unchanged {
        return Ok(Change::Unchanged);
    }

    fs::create_dir_all(&paths.user_bin).map_err(|err| {
        AppError::runtime(format!(
            "사용자 CLI directory 생성 실패: {} ({err})",
            paths.user_bin.display()
        ))
    })?;
    copy_executable_atomically(&paths.source_binary, &paths.installed_binary)?;
    Ok(plan)
}

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

pub(crate) fn validate_clean_targets(paths: &InstallPaths) -> Result<(), AppError> {
    let app_data = absolute_path(&paths.app_data)?;
    let project_root = absolute_path(&paths.project_root)?;
    let project_state = absolute_path(&paths.project_state)?;
    let user_home = absolute_path(&paths.user_home)?;
    let source_binary = absolute_path(&paths.source_binary)?;
    let installed_binary = absolute_path(&paths.installed_binary)?;
    let current_dir = env::current_dir()
        .map_err(|err| AppError::runtime(format!("현재 directory 확인 실패: {err}")))?;
    let resolved_app_data = resolve_existing_path(&app_data);
    let resolved_project_root = resolve_existing_path(&project_root);
    let resolved_project_state = resolve_existing_path(&project_state);
    let resolved_user_home = resolve_existing_path(&user_home);
    let resolved_source_binary = resolve_existing_path(&source_binary);
    let resolved_installed_binary = resolve_existing_path(&installed_binary);
    let resolved_current_dir = resolve_existing_path(&current_dir);

    if project_state.file_name().and_then(|name| name.to_str()) != Some(".rpotato") {
        return Err(AppError::blocked(format!(
            "clean install project-state 경계가 유효하지 않습니다: {}",
            project_state.display()
        )));
    }
    for protected in [&user_home, &project_root] {
        if paths_resolve_equal(&app_data, protected) {
            return Err(AppError::blocked(format!(
                "clean install이 보호 경로를 app-data root로 삭제하려 해 차단했습니다: {}",
                app_data.display()
            )));
        }
    }
    if app_data.parent().is_none()
        || resolved_project_root.starts_with(&resolved_app_data)
        || resolved_user_home.starts_with(&resolved_app_data)
        || resolved_source_binary.starts_with(&resolved_app_data)
        || resolved_installed_binary.starts_with(&resolved_app_data)
        || resolved_current_dir.starts_with(&resolved_app_data)
    {
        return Err(AppError::blocked(format!(
            "clean install app-data 경계가 너무 넓어 차단했습니다: {}",
            app_data.display()
        )));
    }
    if resolved_source_binary.starts_with(&resolved_project_state)
        || resolved_installed_binary.starts_with(&resolved_project_state)
        || resolved_user_home.starts_with(&resolved_project_state)
        || resolved_current_dir.starts_with(&resolved_project_state)
    {
        return Err(AppError::blocked(format!(
            "clean install project-state 안의 보호 경로를 삭제하려 해 차단했습니다: {}",
            project_state.display()
        )));
    }
    Ok(())
}

pub(crate) fn remove_clean_state(paths: &InstallPaths) -> Result<CleanStateResult, AppError> {
    validate_clean_targets(paths)?;
    let app_data_removed = remove_managed_path(&paths.app_data)?;
    let project_state_removed = remove_managed_path(&paths.project_state)?;
    Ok(CleanStateResult {
        app_data_removed,
        project_state_removed,
    })
}

fn user_home() -> Result<PathBuf, AppError> {
    #[cfg(windows)]
    {
        return env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or_else(|| {
                AppError::blocked("사용자 home 경로를 찾지 못해 CLI를 설치할 수 없습니다.")
            });
    }
    #[cfg(not(windows))]
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| AppError::blocked("사용자 home 경로를 찾지 못해 CLI를 설치할 수 없습니다."))
}

fn user_bin_dir(home: &Path) -> Result<PathBuf, AppError> {
    #[cfg(windows)]
    {
        return env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Programs").join("rpotato").join("bin"))
            .ok_or_else(|| {
                AppError::blocked("LOCALAPPDATA를 찾지 못해 Windows CLI를 설치할 수 없습니다.")
            });
    }
    #[cfg(not(windows))]
    {
        Ok(home.join(".local").join("bin"))
    }
}

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
fn unix_path_plan(paths: &InstallPaths) -> (PathBuf, String) {
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
fn resolve_profile_target(profile: &Path) -> Result<PathBuf, AppError> {
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

fn render_managed_profile(existing: &str, block: &str) -> Result<String, AppError> {
    let begins = existing.match_indices(PROFILE_BEGIN).collect::<Vec<_>>();
    let ends = existing.match_indices(PROFILE_END).collect::<Vec<_>>();
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
        ([(begin, _)], [(end, _)]) if begin < end => {
            let suffix_start = existing[*end..]
                .find('\n')
                .map(|offset| end + offset + 1)
                .unwrap_or(existing.len());
            let mut rendered = String::new();
            rendered.push_str(&existing[..*begin]);
            rendered.push_str(block);
            rendered.push('\n');
            rendered.push_str(&existing[suffix_start..]);
            Ok(rendered)
        }
        _ => Err(AppError::blocked(
            "shell profile의 rpotato managed PATH marker가 손상되어 자동 수정하지 않았습니다.",
        )),
    }
}

#[cfg(windows)]
fn ensure_windows_user_path(paths: &InstallPaths) -> Result<PathRegistration, AppError> {
    windows_path_registration(paths, true, WindowsPathScope::User, 1).and_then(
        |mut registrations| {
            registrations
                .pop()
                .ok_or_else(|| AppError::runtime("Windows 사용자 PATH 등록 결과가 없습니다."))
        },
    )
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy)]
enum WindowsPathScope {
    User,
    #[cfg(test)]
    Process,
}

#[cfg(windows)]
impl WindowsPathScope {
    fn powershell_name(self) -> &'static str {
        match self {
            Self::User => "User",
            #[cfg(test)]
            Self::Process => "Process",
        }
    }

    fn owner(self) -> &'static str {
        match self {
            Self::User => "HKCU\\Environment\\Path",
            #[cfg(test)]
            Self::Process => "PowerShell process PATH",
        }
    }
}

#[cfg(windows)]
fn windows_path_registration(
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

fn remove_managed_path(path: &Path) -> Result<bool, AppError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "clean install target 상태 확인 실패: {} ({err})",
                path.display()
            )));
        }
    };
    let result = if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)
    } else if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        return Err(AppError::blocked(format!(
            "clean install target 유형을 삭제할 수 없습니다: {}",
            path.display()
        )));
    };
    result.map(|_| true).map_err(|err| {
        AppError::runtime(format!(
            "clean install 삭제 실패: {} ({err})",
            path.display()
        ))
    })
}

fn equivalent_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn paths_resolve_equal(left: &Path, right: &Path) -> bool {
    equivalent_path(left, right) || left == right
}

fn resolve_existing_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn absolute_path(path: &Path) -> Result<PathBuf, AppError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    env::current_dir()
        .map(|current| current.join(path))
        .map_err(|err| AppError::runtime(format!("현재 directory 확인 실패: {err}")))
}

#[cfg(test)]
#[path = "system_install/tests.rs"]
mod tests;
