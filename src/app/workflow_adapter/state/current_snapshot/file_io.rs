use super::*;
use std::fs::File;
use std::io::Read;

#[cfg(windows)]
use crate::adapters::filesystem::windows_replace;

pub(crate) fn read_regular_file_bounded(
    path: &std::path::Path,
    max_bytes: u64,
    label: &str,
) -> Result<String, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} metadata 실패: {err}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} regular-file/byte budget 불일치"
        )));
    }
    let mut file =
        File::open(path).map_err(|err| AppError::blocked(format!("{label} 열기 실패: {err}")))?;
    validate_open_read_identity(path, &file, label)?;
    let bytes = read_open_file_bounded(&mut file, max_bytes, label)?;
    validate_open_read_identity(path, &file, label)?;
    String::from_utf8(bytes).map_err(|_| AppError::blocked(format!("{label} UTF-8 불일치")))
}

pub(in crate::app::workflow_adapter::state) fn read_open_file_bounded(
    file: &mut File,
    max_bytes: u64,
    label: &str,
) -> Result<Vec<u8>, AppError> {
    let metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle metadata 실패: {err}")))?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} regular-file/byte budget 불일치"
        )));
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(usize::MAX)
            .min(usize::try_from(max_bytes).unwrap_or(usize::MAX)),
    );
    Read::by_ref(file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("{label} 읽기 실패: {err}")))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} byte budget 초과; 증거를 보존했습니다."
        )));
    }
    let after = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 재검증 실패: {err}")))?;
    if !after.is_file() || after.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} read 중 byte budget 변경; 증거를 보존했습니다."
        )));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let same_file = windows_replace::path_refers_to_open_file(path, file)
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() || !same_file {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.len() != file_metadata.len()
    {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}
