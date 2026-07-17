use super::*;

pub(super) fn remove_stale_owner_claims(directory: &Path, context: &str) -> Result<(), AppError> {
    const OWNER_SCAN_LIMIT: usize = 128;
    let mut matched = 0_usize;
    for entry in fs::read_dir(directory)
        .map_err(|err| AppError::runtime(format!("{context} owner claim scan 실패: {err}")))?
    {
        let entry = entry
            .map_err(|err| AppError::runtime(format!("{context} owner claim entry 실패: {err}")))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !name.starts_with("claim-") {
            return Err(AppError::blocked(format!(
                "{context} owner claim namespace 불일치; 증거를 보존했습니다."
            )));
        }
        matched = matched.saturating_add(1);
        if matched > OWNER_SCAN_LIMIT {
            return Err(AppError::blocked(format!(
                "{context} owner claim scan budget 초과; 증거를 보존했습니다."
            )));
        }
        let owner_path = entry.path();
        reject_non_regular_lock_path(&owner_path, context)?;
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&owner_path)
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    AppError::blocked(format!(
                        "{context} lock 차단: owner claim changed during scan"
                    ))
                } else {
                    AppError::blocked(format!("{context} owner claim 열기 실패: {err}"))
                }
            })?;
        validate_open_owner_claim_identity(&owner_path, &owner, context)?;
        match owner.try_lock() {
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(AppError::blocked(format!(
                    "{context} lock 차단: active owner claim"
                )))
            }
            Err(std::fs::TryLockError::Error(err)) => {
                return Err(AppError::runtime(format!(
                    "{context} owner claim 검사 실패: {err}"
                )))
            }
            Ok(()) => {
                validate_open_owner_claim_identity(&owner_path, &owner, context)?;
                drop(owner);
                match fs::remove_file(&owner_path) {
                    Ok(()) => {}
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => {
                        return Err(AppError::blocked(format!(
                            "{context} stale owner claim 정리 실패: {err}"
                        )))
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn open_owner_namespace_guard(
    directory: &Path,
    context: &str,
) -> Result<(PathBuf, File), AppError> {
    let guard = File::open(directory)
        .map_err(|err| AppError::runtime(format!("{context} owner namespace 열기 실패: {err}")))?;
    Ok((directory.to_path_buf(), guard))
}

#[cfg(not(unix))]
pub(super) fn open_owner_namespace_guard(
    directory: &Path,
    context: &str,
) -> Result<(PathBuf, File), AppError> {
    let guard_path = directory
        .parent()
        .ok_or_else(|| AppError::runtime(format!("{context} owner namespace parent 누락")))?
        .join("namespace.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    let guard = options
        .open(&guard_path)
        .map_err(|err| AppError::runtime(format!("{context} owner namespace 열기 실패: {err}")))?;
    Ok((guard_path, guard))
}

#[cfg(unix)]
pub(super) fn validate_open_owner_namespace_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path).map_err(|err| {
        AppError::blocked(format!("{context} owner namespace 경로 재검증 실패: {err}"))
    })?;
    let file_metadata = file.metadata().map_err(|err| {
        AppError::blocked(format!("{context} owner namespace handle 검증 실패: {err}"))
    })?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_dir()
        || !file_metadata.is_dir()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{context} owner namespace path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn validate_open_owner_namespace_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    validate_open_lock_identity(path, file, context)
}

fn validate_open_owner_claim_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    match validate_open_lock_identity(path, file, context) {
        Ok(()) => Ok(()),
        Err(error) => match fs::symlink_metadata(path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(AppError::blocked(
                format!("{context} lock 차단: owner claim changed during scan"),
            )),
            _ => Err(error),
        },
    }
}

pub(super) fn owner_claim_directory(lock_path: &Path, context: &str) -> Result<PathBuf, AppError> {
    let parent = lock_path
        .parent()
        .ok_or_else(|| AppError::runtime(format!("{context} lock parent 누락")))?;
    let parent = fs::canonicalize(parent).map_err(|err| {
        AppError::runtime(format!("{context} lock parent canonicalize 실패: {err}"))
    })?;
    let file_name = lock_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::blocked(format!("{context} lock filename 불일치")))?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in parent
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .copied()
        .chain([0])
        .chain(file_name.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let root = std::env::temp_dir().join(format!("rpotato-lease-owner-claims-{hash:016x}"));
    let directory = root.join("claims");
    fs::create_dir_all(&directory).map_err(|err| {
        AppError::runtime(format!("{context} owner claim directory 생성 실패: {err}"))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|err| {
            AppError::runtime(format!("{context} owner claim root 권한 설정 실패: {err}"))
        })?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(|err| {
            AppError::runtime(format!(
                "{context} owner claim directory 권한 설정 실패: {err}"
            ))
        })?;
    }
    for path in [&root, &directory] {
        let metadata = fs::symlink_metadata(path).map_err(|err| {
            AppError::blocked(format!("{context} owner claim directory 검증 실패: {err}"))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AppError::blocked(format!(
                "{context} owner claim directory type 불일치"
            )));
        }
    }
    Ok(directory)
}

pub(super) fn reject_non_regular_lock_path(path: &Path, context: &str) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => {
            Err(AppError::blocked(format!(
                "{context} lock type 불일치; 증거를 보존했습니다."
            )))
        }
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::blocked(format!(
            "{context} lock metadata 실패: {err}"
        ))),
    }
}

#[cfg(unix)]
pub(super) fn validate_open_lock_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} lock 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{context} lock handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.file_type().is_file()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{context} lock path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn validate_open_lock_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} lock 경로 재검증 실패: {err}")))?;
    let same_file = super::super::windows_replace::path_refers_to_open_file(path, file)
        .map_err(|err| AppError::blocked(format!("{context} lock handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.file_type().is_file() || !same_file
    {
        return Err(AppError::blocked(format!(
            "{context} lock path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(super) fn validate_open_lock_identity(
    path: &Path,
    file: &File,
    context: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} lock 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{context} lock handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.file_type().is_file()
        || path_metadata.len() != file_metadata.len()
    {
        return Err(AppError::blocked(format!(
            "{context} lock path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}
