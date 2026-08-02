use super::super::{
    read_stdin_line, resolve_choice, zeroize_string, TerminalChoice, TerminalFault,
    TerminalSuggestion, TestTerminalFault,
};
use std::ffi::c_void;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};

type Handle = *mut c_void;
const STD_INPUT_HANDLE: u32 = -10i32 as u32;
const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
const ENABLE_ECHO_INPUT: u32 = 0x0004;
const CTRL_C_EVENT: u32 = 0;
const CTRL_BREAK_EVENT: u32 = 1;
const CTRL_CLOSE_EVENT: u32 = 2;
const CTRL_LOGOFF_EVENT: u32 = 5;
const CTRL_SHUTDOWN_EVENT: u32 = 6;

#[repr(C)]
#[derive(Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
struct ConsoleScreenBufferInfo {
    size: Coord,
    cursor_position: Coord,
    attributes: u16,
    window: SmallRect,
    maximum_window_size: Coord,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(kind: u32) -> Handle;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
    fn GetConsoleScreenBufferInfo(handle: Handle, info: *mut ConsoleScreenBufferInfo) -> i32;
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}

static SIGNAL_ECHO_RESTORE_ARMED: AtomicBool = AtomicBool::new(false);
static SIGNAL_ECHO_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static SIGNAL_ECHO_ORIGINAL: AtomicU32 = AtomicU32::new(0);
static REQUEST_CANCEL_ARMED: AtomicBool = AtomicBool::new(false);
static REQUEST_CANCELLED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn capture_request_cancel(control: u32) -> i32 {
    if control == CTRL_C_EVENT && REQUEST_CANCEL_ARMED.load(Ordering::Acquire) {
        REQUEST_CANCELLED.store(true, Ordering::Release);
        return 1;
    }
    0
}

pub fn begin_request_cancel_capture() -> Result<(), TerminalFault> {
    if REQUEST_CANCEL_ARMED.swap(true, Ordering::SeqCst) {
        return Err(TerminalFault::ModeRead);
    }
    REQUEST_CANCELLED.store(false, Ordering::Release);
    // SAFETY: the callback uses the Windows console control handler ABI.
    if unsafe { SetConsoleCtrlHandler(Some(capture_request_cancel), 1) } == 0 {
        REQUEST_CANCEL_ARMED.store(false, Ordering::SeqCst);
        return Err(TerminalFault::ModeRead);
    }
    Ok(())
}

pub fn request_cancelled() -> bool {
    REQUEST_CANCELLED.load(Ordering::Acquire)
}

pub fn end_request_cancel_capture() {
    if !REQUEST_CANCEL_ARMED.swap(false, Ordering::SeqCst) {
        return;
    }
    // SAFETY: removes the exact callback installed when capture began.
    let _ = unsafe { SetConsoleCtrlHandler(Some(capture_request_cancel), 0) };
    REQUEST_CANCELLED.store(false, Ordering::Release);
}

unsafe extern "system" fn restore_echo_before_console_exit(control: u32) -> i32 {
    if matches!(
        control,
        CTRL_C_EVENT
            | CTRL_BREAK_EVENT
            | CTRL_CLOSE_EVENT
            | CTRL_LOGOFF_EVENT
            | CTRL_SHUTDOWN_EVENT
    ) && SIGNAL_ECHO_RESTORE_ARMED.swap(false, Ordering::SeqCst)
    {
        let handle = SIGNAL_ECHO_HANDLE.load(Ordering::SeqCst);
        let original = SIGNAL_ECHO_ORIGINAL.load(Ordering::SeqCst);
        if !handle.is_null() {
            // SAFETY: handle and mode were captured from GetConsoleMode before arming.
            let _ = unsafe { SetConsoleMode(handle, original) };
        }
    }
    0
}

#[cfg(debug_assertions)]
pub fn input_mode() -> Result<u32, TerminalFault> {
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let mut mode = 0;
    // SAFETY: mode points to writable storage and handle is the process stdin handle.
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        return Err(TerminalFault::ModeRead);
    }
    Ok(mode)
}

pub fn dimensions() -> Result<(u16, u16), TerminalFault> {
    // SAFETY: GetStdHandle has no Rust-side preconditions.
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let mut info = std::mem::MaybeUninit::<ConsoleScreenBufferInfo>::uninit();
    // SAFETY: info is writable and initialized by the API on success.
    if unsafe { GetConsoleScreenBufferInfo(handle, info.as_mut_ptr()) } == 0 {
        return Err(TerminalFault::SizeRead);
    }
    // SAFETY: the preceding API call succeeded.
    let info = unsafe { info.assume_init() };
    let cols = info.window.right - info.window.left + 1;
    let rows = info.window.bottom - info.window.top + 1;
    let cols = u16::try_from(cols).map_err(|_| TerminalFault::SizeRead)?;
    let rows = u16::try_from(rows).map_err(|_| TerminalFault::SizeRead)?;
    if cols == 0 || rows == 0 {
        return Err(TerminalFault::SizeRead);
    }
    Ok((cols, rows))
}
pub fn read_input_with_suggestions(
    _suggestions: &[TerminalSuggestion],
    _base_frame: &str,
    _state: Option<super::super::live_input::State>,
) -> Result<super::super::live_input::ReadOutcome, TerminalFault> {
    read_stdin_line(TerminalFault::LineRead).map(super::super::live_input::ReadOutcome::from_line)
}
pub fn choose(_title: &str, choices: &[TerminalChoice]) -> Result<Option<String>, TerminalFault> {
    read_stdin_line(TerminalFault::LineRead)
        .map(|input| input.and_then(|input| resolve_choice(choices, &input)))
}

pub fn read_secret() -> Result<Option<String>, TerminalFault> {
    super::super::inject_test_fault(TestTerminalFault::ModeRead, TerminalFault::ModeRead)?;
    // SAFETY: GetStdHandle has no Rust-side preconditions.
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let mut original = 0;
    if unsafe { GetConsoleMode(handle, &mut original) } == 0 {
        return Err(TerminalFault::ModeRead);
    }
    let _signal_restore = SignalEchoRestore::install(handle, original)?;
    super::super::inject_test_fault(TestTerminalFault::NoEchoSet, TerminalFault::NoEchoSet)?;
    // SAFETY: handle and mode came from the console API.
    if unsafe { SetConsoleMode(handle, original & !ENABLE_ECHO_INPUT) } == 0 {
        return Err(TerminalFault::NoEchoSet);
    }
    let mut restore = EchoRestore {
        handle,
        original,
        restored: false,
    };
    let value = match super::super::inject_test_fault(
        TestTerminalFault::SecretRead,
        TerminalFault::SecretRead,
    ) {
        Ok(()) => read_stdin_line(TerminalFault::SecretRead),
        Err(fault) => Err(fault),
    };
    let restored = restore.restore();
    let _ = io::stdout().write_all(b"\n");
    if !restored {
        if let Ok(Some(secret)) = value {
            zeroize_string(secret);
        }
        return Err(TerminalFault::EchoRestore);
    }
    value
}

struct SignalEchoRestore {
    installed: bool,
}

impl SignalEchoRestore {
    fn install(handle: Handle, original: u32) -> Result<Self, TerminalFault> {
        if SIGNAL_ECHO_RESTORE_ARMED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(TerminalFault::ModeRead);
        }
        SIGNAL_ECHO_HANDLE.store(handle, Ordering::SeqCst);
        SIGNAL_ECHO_ORIGINAL.store(original, Ordering::SeqCst);
        // SAFETY: the callback uses the Windows console control handler ABI.
        if unsafe { SetConsoleCtrlHandler(Some(restore_echo_before_console_exit), 1) } == 0 {
            SIGNAL_ECHO_RESTORE_ARMED.store(false, Ordering::SeqCst);
            SIGNAL_ECHO_HANDLE.store(std::ptr::null_mut(), Ordering::SeqCst);
            return Err(TerminalFault::ModeRead);
        }
        Ok(Self { installed: true })
    }

    fn disarm(&mut self) {
        if !self.installed {
            return;
        }
        SIGNAL_ECHO_RESTORE_ARMED.store(false, Ordering::SeqCst);
        // SAFETY: removes the exact callback installed by this prompt.
        let _ = unsafe { SetConsoleCtrlHandler(Some(restore_echo_before_console_exit), 0) };
        SIGNAL_ECHO_HANDLE.store(std::ptr::null_mut(), Ordering::SeqCst);
        self.installed = false;
    }
}

impl Drop for SignalEchoRestore {
    fn drop(&mut self) {
        self.disarm();
    }
}

struct EchoRestore {
    handle: Handle,
    original: u32,
    restored: bool,
}

impl EchoRestore {
    fn restore(&mut self) -> bool {
        if self.restored {
            return true;
        }
        // SAFETY: handle and original were returned by the console API.
        let ok = unsafe { SetConsoleMode(self.handle, self.original) } != 0;
        self.restored = ok;
        ok
    }
}

impl Drop for EchoRestore {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
