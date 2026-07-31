use super::super::{
    read_stdin_line, zeroize_string, TerminalChoice, TerminalFault, TerminalSuggestion,
    TestTerminalFault,
};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const STDIN_FILENO: i32 = 0;
const STDOUT_FILENO: i32 = 1;
const TCSANOW: i32 = 0;
const ECHO: TcFlag = 0x0000_0008;
#[cfg(target_os = "linux")]
const ISIG: TcFlag = 0x0000_0001;
#[cfg(target_os = "macos")]
const ISIG: TcFlag = 0x0000_0080;
#[cfg(target_os = "linux")]
const ICANON: TcFlag = 0x0000_0002;
#[cfg(target_os = "macos")]
const ICANON: TcFlag = 0x0000_0100;
#[cfg(target_os = "linux")]
const VTIME: usize = 5;
#[cfg(target_os = "linux")]
const VMIN: usize = 6;
#[cfg(target_os = "macos")]
const VMIN: usize = 16;
#[cfg(target_os = "macos")]
const VTIME: usize = 17;
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;
const SIG_ERR: usize = usize::MAX;

#[cfg(target_os = "linux")]
type TcFlag = u32;
#[cfg(target_os = "macos")]
type TcFlag = u64;
#[cfg(target_os = "linux")]
const TIOCGWINSZ: usize = 0x5413;
#[cfg(target_os = "macos")]
const TIOCGWINSZ: usize = 0x4008_7468;

#[repr(C)]
#[derive(Clone, Copy)]
struct WinSize {
    rows: u16,
    cols: u16,
    xpixel: u16,
    ypixel: u16,
}

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    c_iflag: u64,
    c_oflag: u64,
    c_cflag: u64,
    c_lflag: u64,
    c_cc: [u8; 20],
    c_ispeed: u64,
    c_ospeed: u64,
}

unsafe extern "C" {
    fn ioctl(fd: i32, request: usize, ...) -> i32;
    fn tcgetattr(fd: i32, termios: *mut Termios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios: *const Termios) -> i32;
    fn signal(signal: i32, handler: usize) -> usize;
    fn _exit(status: i32) -> !;
}

static SIGNAL_ECHO_RESTORE_ARMED: AtomicBool = AtomicBool::new(false);
static mut SIGNAL_ECHO_ORIGINAL: std::mem::MaybeUninit<Termios> = std::mem::MaybeUninit::uninit();
static REQUEST_CANCEL_ARMED: AtomicBool = AtomicBool::new(false);
static REQUEST_CANCELLED: AtomicBool = AtomicBool::new(false);
static REQUEST_CANCEL_PREVIOUS_HANDLER: AtomicUsize = AtomicUsize::new(0);

extern "C" fn capture_request_cancel(_signal_number: i32) {
    REQUEST_CANCELLED.store(true, Ordering::Release);
}

pub fn begin_request_cancel_capture() -> Result<(), TerminalFault> {
    if REQUEST_CANCEL_ARMED.swap(true, Ordering::SeqCst) {
        return Err(TerminalFault::ModeRead);
    }
    REQUEST_CANCELLED.store(false, Ordering::Release);
    // SAFETY: the handler only stores to an atomic and uses the C signal ABI.
    let previous = unsafe { signal(SIGINT, capture_request_cancel as *const () as usize) };
    if previous == SIG_ERR {
        REQUEST_CANCEL_ARMED.store(false, Ordering::SeqCst);
        return Err(TerminalFault::ModeRead);
    }
    REQUEST_CANCEL_PREVIOUS_HANDLER.store(previous, Ordering::Release);
    Ok(())
}

pub fn request_cancelled() -> bool {
    REQUEST_CANCELLED.load(Ordering::Acquire)
}

pub fn end_request_cancel_capture() {
    if !REQUEST_CANCEL_ARMED.swap(false, Ordering::SeqCst) {
        return;
    }
    let previous = REQUEST_CANCEL_PREVIOUS_HANDLER.load(Ordering::Acquire);
    // SAFETY: restores the exact disposition returned when capture began.
    let _ = unsafe { signal(SIGINT, previous) };
    REQUEST_CANCELLED.store(false, Ordering::Release);
}

extern "C" fn restore_echo_before_signal_exit(signal_number: i32) {
    if SIGNAL_ECHO_RESTORE_ARMED.swap(false, Ordering::SeqCst) {
        // SAFETY: the slot is initialized before the handler is installed and remains
        // immutable while armed. The handler only restores the controlling TTY.
        let _ = unsafe {
            tcsetattr(
                STDIN_FILENO,
                TCSANOW,
                std::ptr::addr_of!(SIGNAL_ECHO_ORIGINAL).cast::<Termios>(),
            )
        };
    }
    // SAFETY: _exit terminates immediately after the terminal restoration attempt.
    unsafe { _exit(128_i32.saturating_add(signal_number)) }
}

pub fn dimensions() -> Result<(u16, u16), TerminalFault> {
    let mut size = WinSize {
        rows: 0,
        cols: 0,
        xpixel: 0,
        ypixel: 0,
    };
    // SAFETY: `size` is a valid writable WinSize and stdout is not closed by this call.
    let result = unsafe { ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut size) };
    if result != 0 || size.cols == 0 || size.rows == 0 {
        return Err(TerminalFault::SizeRead);
    }
    Ok((size.cols, size.rows))
}
pub fn read_input_with_suggestions(
    suggestions: &[TerminalSuggestion],
    base_frame: &str,
    state: Option<super::super::live_input::State>,
) -> Result<super::super::live_input::ReadOutcome, TerminalFault> {
    with_live_mode(|width| super::super::live_input::read(suggestions, width, base_frame, state))
}
pub fn choose(title: &str, choices: &[TerminalChoice]) -> Result<Option<String>, TerminalFault> {
    with_live_mode(|width| super::super::live_input::choose(title, choices, width))
}

fn with_live_mode<T>(
    operation: impl FnOnce(usize) -> Result<T, TerminalFault>,
) -> Result<T, TerminalFault> {
    let mut original = std::mem::MaybeUninit::<Termios>::uninit();
    if unsafe { tcgetattr(STDIN_FILENO, original.as_mut_ptr()) } != 0 {
        return Err(TerminalFault::ModeRead);
    }
    // SAFETY: the preceding tcgetattr call succeeded.
    let original = unsafe { original.assume_init() };
    let _signal_restore = SignalEchoRestore::install(original)?;
    let mut live = original;
    live.c_lflag &= !(ECHO | ICANON | ISIG);
    // A short inter-byte timeout lets the line editor distinguish a standalone Escape
    // key from the prefix of CSI/SS3 navigation sequences.
    live.c_cc[VMIN] = 0;
    live.c_cc[VTIME] = 1;
    // SAFETY: both termios pointers are valid for the duration of each call.
    if unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &live) } != 0 {
        return Err(TerminalFault::NoEchoSet);
    }

    let mut restore = EchoRestore {
        original,
        restored: false,
    };
    let width = dimensions().map(|(columns, _)| usize::from(columns))?;
    let value = operation(width);
    if !restore.restore() {
        return Err(TerminalFault::EchoRestore);
    }
    value
}

pub fn read_secret() -> Result<Option<String>, TerminalFault> {
    super::super::inject_test_fault(TestTerminalFault::ModeRead, TerminalFault::ModeRead)?;
    let mut original = std::mem::MaybeUninit::<Termios>::uninit();
    // SAFETY: tcgetattr initializes the output on success.
    if unsafe { tcgetattr(STDIN_FILENO, original.as_mut_ptr()) } != 0 {
        return Err(TerminalFault::ModeRead);
    }
    // SAFETY: the preceding tcgetattr call succeeded.
    let original = unsafe { original.assume_init() };
    let _signal_restore = SignalEchoRestore::install(original)?;
    let mut hidden = original;
    hidden.c_lflag &= !ECHO;
    super::super::inject_test_fault(TestTerminalFault::NoEchoSet, TerminalFault::NoEchoSet)?;
    // SAFETY: both termios pointers are valid for the duration of each call.
    if unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &hidden) } != 0 {
        return Err(TerminalFault::NoEchoSet);
    }

    let mut restore = EchoRestore {
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
    previous_sigint: usize,
    previous_sigterm: usize,
    installed: bool,
}

impl SignalEchoRestore {
    fn install(original: Termios) -> Result<Self, TerminalFault> {
        if SIGNAL_ECHO_RESTORE_ARMED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(TerminalFault::ModeRead);
        }
        // SAFETY: the atomic guard gives this prompt exclusive ownership of the slot.
        unsafe {
            std::ptr::addr_of_mut!(SIGNAL_ECHO_ORIGINAL).write(std::mem::MaybeUninit::new(original))
        };
        // SAFETY: the handler has the C signal ABI and SIGINT is a POSIX signal.
        let handler = restore_echo_before_signal_exit as *const () as usize;
        let previous_sigint = unsafe { signal(SIGINT, handler) };
        if previous_sigint == SIG_ERR {
            SIGNAL_ECHO_RESTORE_ARMED.store(false, Ordering::SeqCst);
            return Err(TerminalFault::ModeRead);
        }
        // SAFETY: the handler has the C signal ABI and SIGTERM is a POSIX signal.
        let previous_sigterm = unsafe { signal(SIGTERM, handler) };
        if previous_sigterm == SIG_ERR {
            // SAFETY: previous_sigint was returned by signal for SIGINT.
            let _ = unsafe { signal(SIGINT, previous_sigint) };
            SIGNAL_ECHO_RESTORE_ARMED.store(false, Ordering::SeqCst);
            return Err(TerminalFault::ModeRead);
        }
        Ok(Self {
            previous_sigint,
            previous_sigterm,
            installed: true,
        })
    }

    fn disarm(&mut self) {
        if !self.installed {
            return;
        }
        SIGNAL_ECHO_RESTORE_ARMED.store(false, Ordering::SeqCst);
        // SAFETY: both values were returned by signal for their matching signals.
        let _ = unsafe { signal(SIGINT, self.previous_sigint) };
        let _ = unsafe { signal(SIGTERM, self.previous_sigterm) };
        self.installed = false;
    }
}

impl Drop for SignalEchoRestore {
    fn drop(&mut self) {
        self.disarm();
    }
}

struct EchoRestore {
    original: Termios,
    restored: bool,
}

impl EchoRestore {
    fn restore(&mut self) -> bool {
        if self.restored {
            return true;
        }
        // SAFETY: original is a captured valid termios value for stdin.
        let ok = unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &self.original) } == 0;
        self.restored = ok;
        ok
    }
}

impl Drop for EchoRestore {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
