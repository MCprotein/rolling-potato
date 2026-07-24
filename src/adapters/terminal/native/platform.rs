#[cfg(any(target_os = "linux", target_os = "macos"))]
mod imp {
    use super::super::{
        read_stdin_line, zeroize_string, TerminalChoice, TerminalFault, TerminalSuggestion,
        TestTerminalFault,
    };
    use std::io::{self, Write};
    use std::sync::atomic::{AtomicBool, Ordering};

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

    pub fn read_line_with_suggestions(
        suggestions: &[TerminalSuggestion],
        base_frame: &str,
    ) -> Result<Option<String>, TerminalFault> {
        with_live_mode(|width| super::super::live_input::read(suggestions, width, base_frame))
    }

    pub fn choose(
        title: &str,
        choices: &[TerminalChoice],
    ) -> Result<Option<String>, TerminalFault> {
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
mod imp {
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

    pub fn read_line_with_suggestions(
        _suggestions: &[TerminalSuggestion],
        _base_frame: &str,
    ) -> Result<Option<String>, TerminalFault> {
        read_stdin_line(TerminalFault::LineRead)
    }

    pub fn choose(
        _title: &str,
        choices: &[TerminalChoice],
    ) -> Result<Option<String>, TerminalFault> {
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
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod imp {
    use super::super::{TerminalChoice, TerminalFault, TerminalSuggestion};

    pub fn dimensions() -> Result<(u16, u16), TerminalFault> {
        Err(TerminalFault::SizeRead)
    }

    pub fn read_secret() -> Result<Option<String>, TerminalFault> {
        Err(TerminalFault::ModeRead)
    }

    pub fn read_line_with_suggestions(
        _suggestions: &[TerminalSuggestion],
        _base_frame: &str,
    ) -> Result<Option<String>, TerminalFault> {
        Err(TerminalFault::ModeRead)
    }

    pub fn choose(
        _title: &str,
        _choices: &[TerminalChoice],
    ) -> Result<Option<String>, TerminalFault> {
        Err(TerminalFault::ModeRead)
    }
}

pub(super) use imp::{choose, dimensions, read_line_with_suggestions, read_secret};
pub(super) const LIVE_INPUT: bool = cfg!(any(target_os = "linux", target_os = "macos"));
