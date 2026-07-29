use super::*;

#[cfg(windows)]
mod windows {
    use super::*;
    use std::cell::RefCell;
    use std::ffi::{c_void, OsStr};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::rc::Rc;

    include!("windows/ffi.rs");
    include!("windows/session.rs");
    include!("windows/pty.rs");
    include!("windows/process.rs");
}

#[cfg(windows)]
pub use windows::NativePty;
