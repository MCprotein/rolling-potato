use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(windows))]
use std::fs::File;
#[cfg(windows)]
use std::path::PathBuf;

use crate::foundation::error::AppError;

pub(crate) fn atomic_replace_bytes(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("atomic write parent path 없음"))?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "atomic write directory 생성 실패: {} ({err})",
            parent.display()
        ))
    })?;
    let temporary = path.with_extension(format!("tmp.{}.{}", std::process::id(), now_ms()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|err| {
        AppError::runtime(format!(
            "atomic temp 생성 실패: {} ({err})",
            temporary.display()
        ))
    })?;
    if let Ok(metadata) = fs::metadata(path) {
        file.set_permissions(metadata.permissions())
            .map_err(|err| AppError::runtime(format!("atomic temp permission 복사 실패: {err}")))?;
    }
    file.write_all(bytes)
        .map_err(|err| AppError::runtime(format!("atomic temp write 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("atomic temp sync 실패: {err}")))?;
    drop(file);
    replace_file(&temporary, path).map_err(|err| {
        let _ = fs::remove_file(&temporary);
        AppError::runtime(format!(
            "atomic replace 실패: {} -> {} ({err})",
            temporary.display(),
            path.display()
        ))
    })?;
    sync_parent(path)
}

#[cfg(not(windows))]
pub(crate) fn replace_file(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
pub(crate) fn replace_file(source: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    type Bool = i32;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> Bool;
    }
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let source = canonical_windows_parent_join(source)?
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = canonical_windows_parent_join(target)?
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both pointers reference NUL-terminated buffers that remain alive for the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn canonical_windows_parent_join(path: &Path) -> std::io::Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "replacement path has no file name",
        )
    })?;
    Ok(fs::canonicalize(parent)?.join(file_name))
}

#[cfg(not(windows))]
pub(crate) fn sync_parent(path: &Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("sync parent path 없음"))?;
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|err| {
            AppError::runtime(format!(
                "parent directory sync 실패: {} ({err})",
                parent.display()
            ))
        })
}

#[cfg(windows)]
pub(crate) fn sync_parent(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
