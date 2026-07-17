use crate::foundation::error::AppError;
use std::fs::File;

pub(super) mod unix_open_flags {
    #[cfg(target_os = "macos")]
    pub const READ_DIRECTORY_NOFOLLOW: i32 = 0x0010_0000 | 0x0000_0100 | 0x0100_0000;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    pub const READ_DIRECTORY_NOFOLLOW: i32 = 0x0000_4000 | 0x0000_8000 | 0x0008_0000;
    #[cfg(all(
        not(target_os = "macos"),
        not(all(target_os = "linux", target_arch = "aarch64"))
    ))]
    pub const READ_DIRECTORY_NOFOLLOW: i32 = 0x0001_0000 | 0x0002_0000 | 0x0008_0000;
    #[cfg(target_os = "macos")]
    pub const READ_FILE_NOFOLLOW: i32 = 0x0000_0100 | 0x0100_0000;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    pub const READ_FILE_NOFOLLOW: i32 = 0x0000_8000 | 0x0008_0000;
    #[cfg(all(
        not(target_os = "macos"),
        not(all(target_os = "linux", target_arch = "aarch64"))
    ))]
    pub const READ_FILE_NOFOLLOW: i32 = 0x0002_0000 | 0x0008_0000;
    #[cfg(target_os = "macos")]
    pub const WRITE_CREATE_NEW_NOFOLLOW: i32 =
        0x0000_0001 | 0x0000_0200 | 0x0000_0800 | 0x0000_0100 | 0x0100_0000;
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    pub const WRITE_CREATE_NEW_NOFOLLOW: i32 =
        0x0000_0001 | 0x0000_0040 | 0x0000_0080 | 0x0000_8000 | 0x0008_0000;
    #[cfg(all(
        not(target_os = "macos"),
        not(all(target_os = "linux", target_arch = "aarch64"))
    ))]
    pub const WRITE_CREATE_NEW_NOFOLLOW: i32 =
        0x0000_0001 | 0x0000_0040 | 0x0000_0080 | 0x0002_0000 | 0x0008_0000;
}

pub(super) fn openat_file(
    directory: &File,
    name: &str,
    flags: i32,
    mode: u32,
    context: &str,
) -> Result<File, AppError> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    unsafe extern "C" {
        fn openat(directory_fd: i32, path: *const std::ffi::c_char, flags: i32, mode: u32) -> i32;
    }
    let name =
        CString::new(name).map_err(|_| AppError::blocked(format!("{context} NUL path 차단")))?;
    // SAFETY: directory is an owned live descriptor, name is NUL-terminated, and mode is
    // supplied for both creating and non-creating calls (ignored by the latter).
    let descriptor = unsafe { openat(directory.as_raw_fd(), name.as_ptr(), flags, mode) };
    if descriptor < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(2) {
            return Err(AppError::blocked(format!("{context} (not found)")));
        }
        return Err(AppError::blocked(format!("{context} 실패: {error}")));
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

pub(super) fn mkdirat_directory(directory: &File, name: &str, mode: u32) -> Result<(), AppError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn mkdirat(directory_fd: i32, path: *const std::ffi::c_char, mode: u32) -> i32;
    }
    let name = CString::new(name)
        .map_err(|_| AppError::blocked("prepared rollback mkdir NUL path 차단"))?;
    // SAFETY: the path is NUL-terminated and resolved beneath the retained directory fd.
    if unsafe { mkdirat(directory.as_raw_fd(), name.as_ptr(), mode) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(17) {
            return Err(AppError::blocked(format!(
                "prepared rollback parent create 실패: {error}"
            )));
        }
    }
    Ok(())
}

pub(super) fn dir_linkat(directory: &File, from: &str, to: &str) -> Result<(), AppError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn linkat(
            old_directory_fd: i32,
            old_path: *const std::ffi::c_char,
            new_directory_fd: i32,
            new_path: *const std::ffi::c_char,
            flags: i32,
        ) -> i32;
    }
    let from = CString::new(from).map_err(|_| AppError::blocked("source link NUL path 차단"))?;
    let to = CString::new(to).map_err(|_| AppError::blocked("source link NUL path 차단"))?;
    // SAFETY: both paths are NUL-terminated and resolved relative to the same live directory.
    if unsafe {
        linkat(
            directory.as_raw_fd(),
            from.as_ptr(),
            directory.as_raw_fd(),
            to.as_ptr(),
            0,
        )
    } != 0
    {
        return Err(AppError::blocked(format!(
            "source recovery create-new link 실패: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

pub(super) fn dir_unlinkat(directory: &File, name: &str) -> Result<(), AppError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn unlinkat(directory_fd: i32, path: *const std::ffi::c_char, flags: i32) -> i32;
    }
    let name = CString::new(name).map_err(|_| AppError::blocked("source unlink NUL path 차단"))?;
    // SAFETY: the path is NUL-terminated and resolved under the retained directory descriptor.
    if unsafe { unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        return Err(AppError::blocked(format!(
            "source recovery unlink 실패: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}
