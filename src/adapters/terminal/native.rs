use std::io::{self, IsTerminal, Write};

pub(crate) use crate::runtime_core::terminal::{FrameWriteBoundary, TerminalFault, TerminalIo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestTerminalFault {
    SizeRead,
    ModeRead,
    NoEchoSet,
    SecretRead,
    FrameWriteBeforeDispatch,
    FrameWriteAfterDispatch,
}

pub fn validate_native_fault_configuration() -> Result<(), TerminalFault> {
    validate_test_fault_configuration()
}

pub struct NativeTerminal {
    allow_piped_dimensions: bool,
}

impl NativeTerminal {
    pub fn new() -> Self {
        Self {
            allow_piped_dimensions: false,
        }
    }

    pub fn explicit_line_mode() -> Self {
        Self {
            allow_piped_dimensions: true,
        }
    }
}

impl TerminalIo for NativeTerminal {
    fn validate_configuration(&mut self) -> Result<(), TerminalFault> {
        validate_test_fault_configuration()
    }

    fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault> {
        inject_test_fault(TestTerminalFault::SizeRead, TerminalFault::SizeRead)?;
        match platform::dimensions() {
            Ok(size) => Ok(size),
            Err(_) if self.allow_piped_dimensions && !io::stdout().is_terminal() => {
                let columns = std::env::var("COLUMNS")
                    .ok()
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(80);
                let lines = std::env::var("LINES")
                    .ok()
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(24);
                Ok((columns, lines))
            }
            Err(fault) => Err(fault),
        }
    }

    fn read_line(&mut self) -> Result<Option<String>, TerminalFault> {
        read_stdin_line(TerminalFault::LineRead)
    }

    fn read_secret(&mut self) -> Result<Option<String>, TerminalFault> {
        platform::read_secret()
    }

    fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault> {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(frame.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|_| TerminalFault::FrameWrite)
    }

    fn write_frame_at(
        &mut self,
        frame: &str,
        boundary: FrameWriteBoundary,
    ) -> Result<(), TerminalFault> {
        match boundary {
            FrameWriteBoundary::Ordinary => {}
            FrameWriteBoundary::PreDispatch => inject_test_fault(
                TestTerminalFault::FrameWriteBeforeDispatch,
                TerminalFault::FrameWrite,
            )?,
            FrameWriteBoundary::PostDispatch => inject_test_fault(
                TestTerminalFault::FrameWriteAfterDispatch,
                TerminalFault::FrameWrite,
            )?,
        }
        self.write_frame(frame)
    }
}

#[cfg(debug_assertions)]
fn configured_test_fault() -> Result<Option<TestTerminalFault>, TerminalFault> {
    parse_test_fault_value(std::env::var_os("RPOTATO_TEST_TERMINAL_FAULT").as_deref())
}

#[cfg(debug_assertions)]
fn parse_test_fault_value(
    value: Option<&std::ffi::OsStr>,
) -> Result<Option<TestTerminalFault>, TerminalFault> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    match value.to_str() {
        Some("size-read") => Ok(Some(TestTerminalFault::SizeRead)),
        Some("mode-read") => Ok(Some(TestTerminalFault::ModeRead)),
        Some("no-echo-set") => Ok(Some(TestTerminalFault::NoEchoSet)),
        Some("secret-read") => Ok(Some(TestTerminalFault::SecretRead)),
        Some("frame-write-before-dispatch") => {
            Ok(Some(TestTerminalFault::FrameWriteBeforeDispatch))
        }
        Some("frame-write-after-dispatch") => Ok(Some(TestTerminalFault::FrameWriteAfterDispatch)),
        _ => Err(TerminalFault::InvalidFaultConfiguration),
    }
}

#[cfg(debug_assertions)]
fn validate_test_fault_configuration() -> Result<(), TerminalFault> {
    configured_test_fault().map(|_| ())
}

#[cfg(not(debug_assertions))]
#[inline(always)]
fn validate_test_fault_configuration() -> Result<(), TerminalFault> {
    Ok(())
}

#[cfg(debug_assertions)]
fn inject_test_fault(
    expected: TestTerminalFault,
    fault: TerminalFault,
) -> Result<(), TerminalFault> {
    if configured_test_fault()? == Some(expected) {
        return Err(fault);
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
#[inline(always)]
fn inject_test_fault(
    _expected: TestTerminalFault,
    _fault: TerminalFault,
) -> Result<(), TerminalFault> {
    Ok(())
}

fn read_stdin_line(fault: TerminalFault) -> Result<Option<String>, TerminalFault> {
    let mut line = String::new();
    let bytes = io::stdin().read_line(&mut line).map_err(|_| fault)?;
    if bytes == 0 {
        return Ok(None);
    }
    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    Ok(Some(line))
}

fn zeroize_string(value: String) {
    let mut bytes = value.into_bytes();
    for byte in &mut bytes {
        // SAFETY: `byte` is a valid, uniquely borrowed byte in the owned buffer.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod platform {
    use super::{read_stdin_line, zeroize_string, TerminalFault, TestTerminalFault};
    use std::io::{self, Write};
    use std::sync::atomic::{AtomicBool, Ordering};

    const STDIN_FILENO: i32 = 0;
    const STDOUT_FILENO: i32 = 1;
    const TCSANOW: i32 = 0;
    const ECHO: TcFlag = 0x0000_0008;
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
    static mut SIGNAL_ECHO_ORIGINAL: std::mem::MaybeUninit<Termios> =
        std::mem::MaybeUninit::uninit();

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

    pub fn read_secret() -> Result<Option<String>, TerminalFault> {
        super::inject_test_fault(TestTerminalFault::ModeRead, TerminalFault::ModeRead)?;
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
        super::inject_test_fault(TestTerminalFault::NoEchoSet, TerminalFault::NoEchoSet)?;
        // SAFETY: both termios pointers are valid for the duration of each call.
        if unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &hidden) } != 0 {
            return Err(TerminalFault::NoEchoSet);
        }

        let mut restore = EchoRestore {
            original,
            restored: false,
        };
        let value = match super::inject_test_fault(
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
                std::ptr::addr_of_mut!(SIGNAL_ECHO_ORIGINAL)
                    .write(std::mem::MaybeUninit::new(original))
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
}

#[cfg(windows)]
mod platform {
    use super::{read_stdin_line, zeroize_string, TerminalFault, TestTerminalFault};
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
        // SAFETY: GetStdHandle has no Rust-side preconditions.
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

    pub fn read_secret() -> Result<Option<String>, TerminalFault> {
        super::inject_test_fault(TestTerminalFault::ModeRead, TerminalFault::ModeRead)?;
        // SAFETY: GetStdHandle has no Rust-side preconditions.
        let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let mut original = 0;
        // SAFETY: original points to writable mode storage.
        if unsafe { GetConsoleMode(handle, &mut original) } == 0 {
            return Err(TerminalFault::ModeRead);
        }
        let _signal_restore = SignalEchoRestore::install(handle, original)?;
        super::inject_test_fault(TestTerminalFault::NoEchoSet, TerminalFault::NoEchoSet)?;
        // SAFETY: handle and mode came from the console API.
        if unsafe { SetConsoleMode(handle, original & !ENABLE_ECHO_INPUT) } == 0 {
            return Err(TerminalFault::NoEchoSet);
        }
        let mut restore = EchoRestore {
            handle,
            original,
            restored: false,
        };
        let value = match super::inject_test_fault(
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
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use super::TerminalFault;

    pub fn dimensions() -> Result<(u16, u16), TerminalFault> {
        Err(TerminalFault::SizeRead)
    }

    pub fn read_secret() -> Result<Option<String>, TerminalFault> {
        Err(TerminalFault::ModeRead)
    }
}

#[cfg(test)]
pub struct ScriptedTerminal {
    dimensions: std::collections::VecDeque<Result<(u16, u16), TerminalFault>>,
    lines: std::collections::VecDeque<Result<Option<String>, TerminalFault>>,
    secrets: std::collections::VecDeque<Result<Option<String>, TerminalFault>>,
    pub frames: Vec<String>,
    pub frame_fault: Option<TerminalFault>,
}

#[cfg(test)]
impl ScriptedTerminal {
    pub fn new(lines: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            dimensions: std::iter::repeat_n(Ok((80, 24)), 64).collect(),
            lines: lines
                .into_iter()
                .map(|line| Ok(Some(line.to_string())))
                .chain(std::iter::once(Ok(None)))
                .collect(),
            secrets: std::collections::VecDeque::new(),
            frames: Vec::new(),
            frame_fault: None,
        }
    }
}

#[cfg(test)]
impl TerminalIo for ScriptedTerminal {
    fn dimensions(&mut self) -> Result<(u16, u16), TerminalFault> {
        self.dimensions.pop_front().unwrap_or(Ok((80, 24)))
    }

    fn read_line(&mut self) -> Result<Option<String>, TerminalFault> {
        self.lines.pop_front().unwrap_or(Ok(None))
    }

    fn read_secret(&mut self) -> Result<Option<String>, TerminalFault> {
        self.secrets.pop_front().unwrap_or(Ok(None))
    }

    fn write_frame(&mut self, frame: &str) -> Result<(), TerminalFault> {
        if let Some(fault) = self.frame_fault.take() {
            return Err(fault);
        }
        self.frames.push(frame.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_terminal_handles_eof_and_records_frames() {
        let mut terminal = ScriptedTerminal::new(["help", "quit"]);
        assert_eq!(terminal.dimensions().unwrap(), (80, 24));
        assert_eq!(terminal.read_line().unwrap().as_deref(), Some("help"));
        terminal.write_frame("frame\n").unwrap();
        assert_eq!(terminal.read_line().unwrap().as_deref(), Some("quit"));
        assert_eq!(terminal.read_line().unwrap(), None);
        assert_eq!(terminal.frames, ["frame\n"]);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn test_fault_configuration_has_an_exact_closed_value_set() {
        for (value, expected) in [
            ("size-read", TestTerminalFault::SizeRead),
            ("mode-read", TestTerminalFault::ModeRead),
            ("no-echo-set", TestTerminalFault::NoEchoSet),
            ("secret-read", TestTerminalFault::SecretRead),
            (
                "frame-write-before-dispatch",
                TestTerminalFault::FrameWriteBeforeDispatch,
            ),
            (
                "frame-write-after-dispatch",
                TestTerminalFault::FrameWriteAfterDispatch,
            ),
        ] {
            assert_eq!(
                parse_test_fault_value(Some(std::ffi::OsStr::new(value))).unwrap(),
                Some(expected)
            );
        }
        assert_eq!(parse_test_fault_value(None).unwrap(), None);
        assert_eq!(
            parse_test_fault_value(Some(std::ffi::OsStr::new(""))).unwrap(),
            None
        );
        assert_eq!(
            parse_test_fault_value(Some(std::ffi::OsStr::new("unknown"))).unwrap_err(),
            TerminalFault::InvalidFaultConfiguration
        );
    }
}
