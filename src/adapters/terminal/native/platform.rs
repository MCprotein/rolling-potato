mod capability;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "platform/unix.rs"]
mod imp;
#[cfg(windows)]
#[path = "platform/windows.rs"]
mod imp;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
#[path = "platform/unsupported.rs"]
mod imp;

pub(super) use capability::LIVE_INPUT;
pub(super) use imp::{
    begin_request_cancel_capture, choose, dimensions, end_request_cancel_capture,
    read_input_with_suggestions, read_secret, request_cancelled,
};
