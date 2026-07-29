use super::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix {
    use super::*;
    use std::ffi::{c_char, c_int, c_void, CStr, CString};

    #[repr(C)]
    struct WinSize {
        rows: u16,
        cols: u16,
        xpixel: u16,
        ypixel: u16,
    }

    #[cfg(target_os = "linux")]
    const TIOCSWINSZ: usize = 0x5414;
    #[cfg(target_os = "macos")]
    const TIOCSWINSZ: usize = 0x8008_7467;
    #[cfg(target_os = "linux")]
    const O_NONBLOCK: c_int = 0x800;
    #[cfg(target_os = "macos")]
    const O_NONBLOCK: c_int = 0x0004;
    const O_RDWR: c_int = 0x0002;
    #[cfg(target_os = "linux")]
    const O_NOCTTY: c_int = 0x0100;
    #[cfg(target_os = "macos")]
    const O_NOCTTY: c_int = 0x0002_0000;
    #[cfg(target_os = "linux")]
    const TIOCSCTTY: usize = 0x540e;
    #[cfg(target_os = "macos")]
    const TIOCSCTTY: usize = 0x2000_7461;
    #[cfg(target_os = "linux")]
    const VEOF: usize = 4;
    #[cfg(target_os = "macos")]
    const VEOF: usize = 0;
    #[cfg(target_os = "linux")]
    const SIGSTOP: c_int = 19;
    #[cfg(target_os = "macos")]
    const SIGSTOP: c_int = 17;
    const SIGKILL: c_int = 9;
    const SIGTERM: c_int = 15;
    const WNOHANG: c_int = 1;
    const F_GETFL: c_int = 3;
    const F_SETFL: c_int = 4;

    #[cfg(target_os = "linux")]
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Termios {
        input_flags: u32,
        output_flags: u32,
        control_flags: u32,
        local_flags: u32,
        line: u8,
        control_characters: [u8; 32],
        input_speed: u32,
        output_speed: u32,
    }

    #[cfg(target_os = "macos")]
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Termios {
        input_flags: u64,
        output_flags: u64,
        control_flags: u64,
        local_flags: u64,
        control_characters: [u8; 20],
        input_speed: u64,
        output_speed: u64,
    }

    unsafe extern "C" {
        fn posix_openpt(flags: c_int) -> c_int;
        fn grantpt(fd: c_int) -> c_int;
        fn unlockpt(fd: c_int) -> c_int;
        fn ptsname_r(fd: c_int, buffer: *mut c_char, length: usize) -> c_int;
        fn open(path: *const c_char, flags: c_int, ...) -> c_int;
        fn fork() -> c_int;
        fn setsid() -> c_int;
        fn dup2(old_fd: c_int, new_fd: c_int) -> c_int;
        fn execv(path: *const c_char, argv: *const *const c_char) -> c_int;
        fn _exit(status: c_int) -> !;
        fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
        fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize;
        fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
        fn ioctl(fd: c_int, request: usize, ...) -> c_int;
        fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
        fn kill(pid: c_int, signal: c_int) -> c_int;
        fn close(fd: c_int) -> c_int;
        fn tcgetattr(fd: c_int, value: *mut Termios) -> c_int;
    }

    pub struct NativePty {
        pid: c_int,
        master: c_int,
        retained_slave: c_int,
        slave_path: CString,
        original_mode: Termios,
        output: Vec<u8>,
        waited: bool,
        termination_signal: c_int,
    }

    impl NativePty {
        pub fn spawn(columns: u16, rows: u16) -> Self {
            trace_stage(&format!("spawn {columns}x{rows}"));
            let binary = CString::new(env!("CARGO_BIN_EXE_rpotato")).unwrap();
            let argv = [binary.as_ptr(), std::ptr::null()];
            let size = WinSize {
                rows,
                cols: columns,
                xpixel: 0,
                ypixel: 0,
            };
            // SAFETY: flags are valid for posix_openpt and ownership stays in this fixture.
            let master = unsafe { posix_openpt(O_RDWR | O_NOCTTY) };
            assert!(
                master >= 0,
                "posix_openpt failed: {}",
                std::io::Error::last_os_error()
            );
            // SAFETY: master is a valid PTY master descriptor.
            assert_eq!(unsafe { grantpt(master) }, 0, "grantpt failed");
            // SAFETY: master is a valid granted PTY master descriptor.
            assert_eq!(unsafe { unlockpt(master) }, 0, "unlockpt failed");
            let mut slave_name = [0 as c_char; 1024];
            // SAFETY: the buffer is writable and master names a valid unlocked PTY.
            assert_eq!(
                unsafe { ptsname_r(master, slave_name.as_mut_ptr(), slave_name.len()) },
                0,
                "ptsname_r failed"
            );
            // SAFETY: ptsname_r wrote a NUL-terminated path into slave_name.
            let slave_path = unsafe { CStr::from_ptr(slave_name.as_ptr()) };
            let owned_slave_path = slave_path.to_owned();
            // SAFETY: path is NUL terminated and flags open the terminal without stealing it.
            let retained_slave = unsafe { open(slave_path.as_ptr(), O_RDWR | O_NOCTTY) };
            assert!(
                retained_slave >= 0,
                "PTY slave open failed: {}",
                std::io::Error::last_os_error()
            );
            let mut original_mode = unsafe { std::mem::zeroed::<Termios>() };
            // SAFETY: retained_slave is a terminal descriptor and original_mode is writable.
            assert_eq!(
                unsafe { tcgetattr(retained_slave, &mut original_mode) },
                0,
                "tcgetattr before failed"
            );
            // SAFETY: master is valid and size has the platform winsize layout.
            assert_eq!(
                unsafe { ioctl(master, TIOCSWINSZ, &size) },
                0,
                "initial PTY resize failed"
            );
            // SAFETY: fork duplicates the owned descriptors into the child.
            let pid = unsafe { fork() };
            assert!(pid >= 0, "fork failed: {}", std::io::Error::last_os_error());
            if pid == 0 {
                // SAFETY: child becomes a session leader, attaches the slave, and replaces itself.
                unsafe {
                    if setsid() < 0 || ioctl(retained_slave, TIOCSCTTY, 0) < 0 {
                        _exit(126);
                    }
                    if dup2(retained_slave, 0) < 0
                        || dup2(retained_slave, 1) < 0
                        || dup2(retained_slave, 2) < 0
                    {
                        _exit(126);
                    }
                    close(master);
                    if retained_slave > 2 {
                        close(retained_slave);
                    }
                    execv(binary.as_ptr(), argv.as_ptr());
                    _exit(127);
                }
            }
            // Use a separately opened slave description for the parent-side restoration
            // oracle. The child owns the pre-fork description as its controlling terminal;
            // on macOS that description can become unreadable after the session leader exits.
            // SAFETY: slave_path remains NUL terminated in the parent.
            let verification_slave = unsafe { open(slave_path.as_ptr(), O_RDWR | O_NOCTTY) };
            assert!(
                verification_slave >= 0,
                "PTY verification slave open failed: {}",
                std::io::Error::last_os_error()
            );
            // SAFETY: the parent no longer needs its copy of the child's slave description.
            let _ = unsafe { close(retained_slave) };
            // SAFETY: master is a valid PTY descriptor owned by the parent.
            let flags = unsafe { fcntl(master, F_GETFL) };
            assert!(flags >= 0, "PTY flag read failed");
            // SAFETY: F_SETFL accepts the retrieved flags plus O_NONBLOCK.
            assert_eq!(unsafe { fcntl(master, F_SETFL, flags | O_NONBLOCK) }, 0);
            Self {
                pid,
                master,
                retained_slave: verification_slave,
                slave_path: owned_slave_path,
                original_mode,
                output: Vec::new(),
                waited: false,
                termination_signal: SIGTERM,
            }
        }

        pub fn resize(&mut self, columns: u16, rows: u16) {
            let size = WinSize {
                rows,
                cols: columns,
                xpixel: 0,
                ypixel: 0,
            };
            // SAFETY: master is valid and size has the platform winsize layout.
            let result = unsafe { ioctl(self.master, TIOCSWINSZ, &size) };
            assert_eq!(
                result,
                0,
                "PTY resize failed: {}",
                std::io::Error::last_os_error()
            );
        }

        pub fn send(&mut self, input: &str) {
            let mut remaining = input.as_bytes();
            while !remaining.is_empty() {
                // SAFETY: the byte slice is valid for the duration of the write.
                let written = unsafe {
                    write(
                        self.master,
                        remaining.as_ptr().cast::<c_void>(),
                        remaining.len(),
                    )
                };
                assert!(
                    written > 0,
                    "PTY input write failed: {}",
                    std::io::Error::last_os_error()
                );
                remaining = &remaining[usize::try_from(written).unwrap()..];
            }
        }

        pub fn send_eof(&mut self) {
            let eof = self.original_mode.control_characters[VEOF];
            assert_ne!(eof, 0, "PTY VEOF must be configured");
            self.send_bytes(&[eof]);
        }

        pub fn send_signal(&mut self, signal: i32) {
            // SAFETY: pid belongs to this live fixture and signal is supplied by the test.
            assert_eq!(
                unsafe { kill(self.pid, signal) },
                0,
                "PTY signal delivery failed: {}",
                std::io::Error::last_os_error()
            );
        }

        pub fn force_drop_escalation_probe(&mut self) {
            self.termination_signal = SIGSTOP;
        }

        pub fn wait_for(&mut self, needle: &str) -> String {
            trace_stage(&format!("wait for {needle:?}"));
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.drain_available();
                let output = String::from_utf8_lossy(&self.output);
                if output.contains(needle) {
                    trace_stage(&format!("found {needle:?}"));
                    return output.into_owned();
                }
                assert!(
                    Instant::now() < deadline,
                    "PTY output timeout; wanted {needle:?}; got {output}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        pub fn mark(&self) -> usize {
            self.output.len()
        }

        pub fn wait_for_after(&mut self, mark: usize, needle: &str) -> String {
            assert!(
                mark <= self.output.len(),
                "PTY output mark is out of bounds"
            );
            trace_stage(&format!("wait after {mark} for {needle:?}"));
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.drain_available();
                let output = String::from_utf8_lossy(&self.output[mark..]);
                if output.contains(needle) {
                    trace_stage(&format!("found after {mark}: {needle:?}"));
                    return output.into_owned();
                }
                assert!(
                    Instant::now() < deadline,
                    "PTY output timeout after mark; wanted {needle:?}; got {output}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        pub fn wait_for_ordered_after(&mut self, mark: usize, first: &str, second: &str) -> String {
            assert!(
                mark <= self.output.len(),
                "PTY output mark is out of bounds"
            );
            trace_stage(&format!(
                "wait after {mark} for ordered {first:?} then {second:?}"
            ));
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.drain_available();
                let output = String::from_utf8_lossy(&self.output[mark..]);
                let ordered = output.find(first).is_some_and(|first_index| {
                    output[first_index + first.len()..].contains(second)
                });
                if ordered {
                    trace_stage(&format!(
                        "found after {mark}: ordered {first:?} then {second:?}"
                    ));
                    return output.into_owned();
                }
                assert!(
                    Instant::now() < deadline,
                    "PTY ordered output timeout after mark; wanted {first:?} then {second:?}; got {output}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        pub fn finish(mut self) -> String {
            let status = self.wait_for_exit();
            self.waited = true;
            for _ in 0..20 {
                self.drain_available();
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(
                status, 0,
                "PTY child did not exit successfully: status={status}"
            );
            self.assert_mode_restored();
            String::from_utf8_lossy(&self.output).into_owned()
        }

        pub fn finish_failure(mut self) -> String {
            let status = self.wait_for_exit();
            self.waited = true;
            for _ in 0..20 {
                self.drain_available();
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_ne!(status, 0, "PTY child unexpectedly succeeded");
            self.assert_mode_restored();
            String::from_utf8_lossy(&self.output).into_owned()
        }

        fn assert_mode_restored(&self) {
            // Reopen after the session leader exits. On macOS, a slave description that
            // lived through the controlling-session hangup returns EIO from tcgetattr.
            // SAFETY: slave_path is the NUL-terminated path returned by ptsname_r.
            let probe = unsafe { open(self.slave_path.as_ptr(), O_RDWR | O_NOCTTY) };
            assert!(
                probe >= 0,
                "PTY restoration probe open failed: {}",
                std::io::Error::last_os_error()
            );
            let mut current = unsafe { std::mem::zeroed::<Termios>() };
            // SAFETY: probe is a freshly opened terminal descriptor.
            assert_eq!(
                unsafe { tcgetattr(probe, &mut current) },
                0,
                "tcgetattr after child failed"
            );
            // SAFETY: probe is owned by this method and is closed once.
            let _ = unsafe { close(probe) };
            assert_eq!(
                current, self.original_mode,
                "terminal mode was not restored"
            );
        }

        fn send_bytes(&mut self, input: &[u8]) {
            let mut remaining = input;
            while !remaining.is_empty() {
                // SAFETY: the byte slice is valid for the duration of the write.
                let written = unsafe {
                    write(
                        self.master,
                        remaining.as_ptr().cast::<c_void>(),
                        remaining.len(),
                    )
                };
                assert!(
                    written > 0,
                    "PTY input write failed: {}",
                    std::io::Error::last_os_error()
                );
                remaining = &remaining[usize::try_from(written).unwrap()..];
            }
        }

        fn wait_for_exit(&mut self) -> c_int {
            trace_stage("wait for child exit");
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let mut status = -1;
                // SAFETY: pid belongs to this fixture and status is writable.
                let waited = unsafe { waitpid(self.pid, &mut status, WNOHANG) };
                if waited == self.pid {
                    trace_stage(&format!("child exited with status {status}"));
                    return status;
                }
                assert_eq!(
                    waited,
                    0,
                    "waitpid failed: {}",
                    std::io::Error::last_os_error()
                );
                self.drain_available();
                assert!(
                    Instant::now() < deadline,
                    "PTY child exit timeout; output={}",
                    String::from_utf8_lossy(&self.output)
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn reap_until(&mut self, deadline: Instant) -> bool {
            loop {
                let mut status = 0;
                // SAFETY: pid belongs to this fixture while waited is false.
                let waited = unsafe { waitpid(self.pid, &mut status, WNOHANG) };
                if waited == self.pid {
                    self.waited = true;
                    return true;
                }
                if waited < 0 {
                    let error = std::io::Error::last_os_error();
                    if error.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    trace_stage(&format!("child reap completed with waitpid error: {error}"));
                    self.waited = true;
                    return true;
                }
                self.drain_available();
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn terminate_and_reap_bounded(&mut self) {
            trace_stage("terminate live PTY child");
            // SAFETY: pid belongs to this live fixture.
            let _ = unsafe { kill(self.pid, self.termination_signal) };
            if self.reap_until(Instant::now() + Duration::from_millis(500)) {
                return;
            }
            trace_stage("escalate live PTY child to SIGKILL");
            // SAFETY: pid still belongs to this live fixture after the bounded wait.
            let _ = unsafe { kill(self.pid, SIGKILL) };
            let _ = self.reap_until(Instant::now() + Duration::from_millis(500));
        }

        fn drain_available(&mut self) {
            let mut buffer = [0u8; 4096];
            loop {
                // SAFETY: the buffer is writable and master is a valid nonblocking descriptor.
                let count = unsafe {
                    read(
                        self.master,
                        buffer.as_mut_ptr().cast::<c_void>(),
                        buffer.len(),
                    )
                };
                if count <= 0 {
                    break;
                }
                self.output
                    .extend_from_slice(&buffer[..usize::try_from(count).unwrap()]);
            }
        }
    }

    impl Drop for NativePty {
        fn drop(&mut self) {
            if !self.waited {
                self.terminate_and_reap_bounded();
            }
            // SAFETY: master is owned by this fixture and closed exactly once.
            let _ = unsafe { close(self.master) };
            // SAFETY: retained_slave is owned by this fixture and closed exactly once.
            let _ = unsafe { close(self.retained_slave) };
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use unix::NativePty;
