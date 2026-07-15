#![cfg(windows)]

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C)]
#[derive(Default)]
struct ByHandleFileInformation {
    file_attributes: u32,
    creation_time: FileTime,
    last_access_time: FileTime,
    last_write_time: FileTime,
    volume_serial_number: u32,
    file_size_high: u32,
    file_size_low: u32,
    number_of_links: u32,
    file_index_high: u32,
    file_index_low: u32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandle(
        file: *mut c_void,
        information: *mut ByHandleFileInformation,
    ) -> i32;
}

pub(crate) fn path_refers_to_open_file(path: &Path, file: &File) -> io::Result<bool> {
    let path_file = File::open(path)?;
    Ok(open_file_identity(&path_file)? == open_file_identity(file)?)
}

fn open_file_identity(file: &File) -> io::Result<(u32, u64)> {
    let mut information = ByHandleFileInformation::default();
    // SAFETY: `file` owns a live Windows handle and `information` is writable for the call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let file_index =
        (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low);
    Ok((information.volume_serial_number, file_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn path_and_open_handle_identity_uses_stable_windows_file_ids() {
        let root = std::env::temp_dir().join(format!(
            "rpotato-windows-file-identity-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("original");
        let hard_link = root.join("hard-link");
        let other = root.join("other");
        std::fs::write(&original, b"original").unwrap();
        std::fs::hard_link(&original, &hard_link).unwrap();
        std::fs::write(&other, b"other").unwrap();
        let file = File::open(&original).unwrap();

        assert!(path_refers_to_open_file(&original, &file).unwrap());
        assert!(path_refers_to_open_file(&hard_link, &file).unwrap());
        assert!(!path_refers_to_open_file(&other, &file).unwrap());

        drop(file);
        let _ = std::fs::remove_dir_all(root);
    }
}
