use super::*;

pub(super) struct BoundedRegularEntry {
    pub(super) name: String,
    pub(super) path: PathBuf,
}

pub(super) fn bounded_regular_entries(
    directory: &Path,
    max_entries: usize,
    max_bytes: usize,
    allowed_name: impl Fn(&str) -> bool,
) -> Result<Vec<BoundedRegularEntry>, std::io::Error> {
    let mut entries = Vec::new();
    let mut bytes = 0_usize;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| std::io::Error::other("non-UTF-8 recovery entry"))?
            .to_string();
        if !metadata.file_type().is_file()
            || metadata.file_type().is_symlink()
            || !allowed_name(&name)
        {
            return Err(std::io::Error::other("invalid recovery entry"));
        }
        if entries.len() >= max_entries {
            return Err(std::io::Error::other("recovery read bound exceeded"));
        }
        bytes = bytes
            .checked_add(usize::try_from(metadata.len()).unwrap_or(usize::MAX))
            .ok_or_else(|| std::io::Error::other("recovery entry byte overflow"))?;
        if bytes > max_bytes {
            return Err(std::io::Error::other("recovery read bound exceeded"));
        }
        entries.push(BoundedRegularEntry { name, path });
    }
    Ok(entries)
}

pub(in crate::app::workflow_adapter::transition) fn read_regular_utf8_bounded(
    path: &Path,
    max_bytes: usize,
    context: &str,
) -> Result<String, AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} metadata 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.len() > u64::try_from(max_bytes).unwrap_or(u64::MAX)
    {
        return Err(AppError::blocked(format!(
            "{context} regular-file/byte budget 불일치"
        )));
    }
    let mut file = fs::File::open(path)
        .map_err(|err| AppError::blocked(format!("{context} 열기 실패: {err}")))?;
    validate_open_regular_file_identity(path, &file, context)?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(path_metadata.len())
            .unwrap_or(max_bytes)
            .min(max_bytes),
    );
    file.by_ref()
        .take(
            u64::try_from(max_bytes)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("{context} 읽기 실패: {err}")))?;
    if bytes.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{context} byte budget 초과; 증거를 보존했습니다."
        )));
    }
    validate_open_regular_file_identity(path, &file, context)?;
    String::from_utf8(bytes).map_err(|_| AppError::blocked(format!("{context} UTF-8 불일치")))
}

#[cfg(unix)]
fn validate_open_regular_file_identity(
    path: &Path,
    file: &fs::File,
    context: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{context} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{context} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_open_regular_file_identity(
    path: &Path,
    file: &fs::File,
    context: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} 경로 재검증 실패: {err}")))?;
    let same_file = windows_replace::path_refers_to_open_file(path, file)
        .map_err(|err| AppError::blocked(format!("{context} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() || !same_file {
        return Err(AppError::blocked(format!(
            "{context} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn validate_open_regular_file_identity(
    path: &Path,
    file: &fs::File,
    context: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{context} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{context} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.len() != file_metadata.len()
    {
        return Err(AppError::blocked(format!(
            "{context} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

pub(in crate::app::workflow_adapter::transition) fn recovery_work_may_exist() -> bool {
    let lag_directory = paths::projection_lag_dir();
    if directory_has_entry_or_is_suspicious(&lag_directory, |_| true) {
        return true;
    }
    let journal_root = paths::project_state_dir().join("transition-journal");
    let projects = match fs::read_dir(&journal_root) {
        Ok(projects) => projects,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    let mut project_count = 0_usize;
    for project in projects {
        project_count = project_count.saturating_add(1);
        if project_count > MAX_RECOVERY_PROJECT_ENTRIES {
            return true;
        }
        let Ok(project) = project else {
            return true;
        };
        let Ok(metadata) = fs::symlink_metadata(project.path()) else {
            return true;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return true;
        }
        if directory_has_entry_or_is_suspicious(&project.path(), |name| name != "transition.lock") {
            return true;
        }
    }
    false
}

fn directory_has_entry_or_is_suspicious(
    directory: &Path,
    counts_as_recovery_work: impl Fn(&str) -> bool,
) -> bool {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return true;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            return true;
        };
        if counts_as_recovery_work(&name) {
            return true;
        }
    }
    false
}
