use std::path::PathBuf;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static NATIVE_TERMINAL_LOCK: Mutex<()> = Mutex::new(());
static SOURCE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const FIXTURE_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

pub struct NativeTerminalFixture {
    _lock: std::sync::MutexGuard<'static, ()>,
    pub root: PathBuf,
    pub project: PathBuf,
    pub data: PathBuf,
}

pub struct PendingSourceApproval {
    pub workflow_id: String,
    pub proposal_id: String,
    pub approval_token: String,
    pub source: PathBuf,
}

impl NativeTerminalFixture {
    pub fn new(case_name: &str) -> Self {
        let lock = NATIVE_TERMINAL_LOCK.lock().unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rpotato-native-terminal-{case_name}-{}-{nonce}",
            std::process::id()
        ));
        let project = root.join("project");
        let data = root.join("data");
        std::fs::create_dir_all(&project).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_rpotato"))
            .arg("init")
            .env("RPOTATO_PROJECT_ROOT", &project)
            .env("RPOTATO_DATA_HOME", &data)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "native terminal fixture init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        std::env::set_var("RPOTATO_DATA_HOME", &data);
        Self {
            _lock: lock,
            root,
            project,
            data,
        }
    }

    pub fn prepare_source_approval(&self) -> PendingSourceApproval {
        let source_dir = self.project.join("src");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source_name = format!(
            "native_source_{}.rs",
            SOURCE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let relative_source = format!("src/{source_name}");
        let source = source_dir.join(source_name);
        std::fs::write(&source, "pub const VALUE: i32 = 1;\n").unwrap();
        let response = self.root.join("response.txt");
        std::fs::write(
            &response,
            format!(
                "수정 후보를 준비했습니다.\nMODEL ACTION: kind=patch-proposal; source_pointers={relative_source}:1; path={relative_source}; find_hex=31; replace_hex=32; verification=pwd; next_gate=diff-before-write; side_effects=none"
            ),
        )
        .unwrap();
        let calls = self.root.join("calls.txt");
        let backend = self.root.join(if cfg!(windows) {
            "fake-sidecar.exe"
        } else {
            "fake-sidecar"
        });
        let fake_sidecar = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/platform/fake_sidecar.rs");
        let compile = Command::new("rustc")
            .arg("--edition=2021")
            .arg(fake_sidecar)
            .arg("-o")
            .arg(&backend)
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "native fixture fake sidecar compile failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let model = self.root.join("model.gguf");
        std::fs::write(&model, b"fake model").unwrap();
        let port = native_port();
        let command = |args: &[&str]| {
            let label = args.join(" ");
            #[cfg(windows)]
            windows::trace_stage(&format!("run {label}"));
            let output = run_bounded_command(
                Command::new(env!("CARGO_BIN_EXE_rpotato"))
                    .args(args)
                    .env("RPOTATO_PROJECT_ROOT", &self.project)
                    .env("RPOTATO_DATA_HOME", &self.data)
                    .env("RPOTATO_BACKEND_LLAMA_CPP_PATH", &backend)
                    .env("RPOTATO_BACKEND_PORT", port.to_string())
                    .env("RPOTATO_FAKE_REQUEST_MARKER", &calls)
                    .env("RPOTATO_FAKE_RESPONSE_FILE", &response)
                    .env(
                        "RPOTATO_TEST_BACKEND_START_TRACE",
                        self.data.join("logs/backend-start-trace.log"),
                    ),
                &label,
                &self.data,
            );
            #[cfg(windows)]
            windows::trace_stage(&format!("finished {label}"));
            output
        };
        let start = command(&[
            "backend",
            "start",
            "--model",
            model.to_str().unwrap(),
            "--ctx-size",
            "1024",
        ]);
        assert!(
            start.status.success(),
            "native source fixture backend start failed\nstdout={}\nstderr={}\n{}",
            String::from_utf8_lossy(&start.stdout),
            String::from_utf8_lossy(&start.stderr),
            backend_failure_diagnostics(&self.data),
        );
        let run = command(&[
            "skill",
            "run",
            "small-patch",
            "src/lib.rs의 값을 2로 고쳐줘",
        ]);
        let _ = command(&["backend", "stop"]);
        let ledger = std::fs::read_to_string(self.data.join("state/runtime-ledger.jsonl"))
            .unwrap_or_default();
        let ledger_tail = ledger
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            run.status.success(),
            "native source fixture skill run failed\nstdout={}\nstderr={}\nledger tail={ledger_tail}\n{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
            backend_failure_diagnostics(&self.data),
        );
        let report = String::from_utf8(run.stdout).unwrap();
        let field = |key: &str| {
            report
                .lines()
                .find_map(|line| line.strip_prefix(&format!("- {key}: ")))
                .unwrap_or_else(|| panic!("missing {key} in native fixture report"))
                .to_string()
        };
        let approval_token = report
            .lines()
            .find_map(|line| line.strip_prefix("- approval command: rpotato patch approve "))
            .and_then(|line| line.split(" --token ").nth(1))
            .expect("native fixture approval token")
            .to_string();
        PendingSourceApproval {
            workflow_id: field("workflow id"),
            proposal_id: field("proposal id"),
            approval_token,
            source,
        }
    }

    #[cfg(windows)]
    pub fn current_session_id(&self) -> String {
        let body = std::fs::read_to_string(self.data.join("state/current-state.json")).unwrap();
        body.split("\"session_id\"")
            .nth(1)
            .and_then(|tail| tail.split_once(':').map(|(_, value)| value))
            .map(str::trim_start)
            .and_then(|tail| tail.strip_prefix('"'))
            .and_then(|tail| tail.split('"').next())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| panic!("current-state session_id missing: {body}"))
            .to_string()
    }
}

fn run_bounded_command(command: &mut Command, label: &str, data: &std::path::Path) -> Output {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "rpotato-native-terminal-output-{}-{nonce}",
        std::process::id()
    ));
    let stdout_path = base.with_extension("stdout");
    let stderr_path = base.with_extension("stderr");
    command
        .stdout(Stdio::from(std::fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr_path).unwrap()));
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + FIXTURE_COMMAND_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let status = child.wait().unwrap();
                let output = captured_command_output(&stdout_path, &stderr_path, status);
                panic!(
                    "native fixture command timeout after {:?}: {label}\nstdout={}\nstderr={}\n{}",
                    FIXTURE_COMMAND_TIMEOUT,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                    backend_failure_diagnostics(data),
                );
            }
            Err(error) => panic!("native fixture command wait failed: {label}: {error}"),
        }
    };
    captured_command_output(&stdout_path, &stderr_path, status)
}

fn backend_failure_diagnostics(data: &std::path::Path) -> String {
    let mut diagnostics = Vec::new();
    let logs = data.join("logs");
    if let Ok(entries) = std::fs::read_dir(&logs) {
        let mut paths = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        paths.sort();
        for path in paths {
            diagnostics.push(format!(
                "log {}:\n{}",
                path.display(),
                String::from_utf8_lossy(&std::fs::read(&path).unwrap_or_default())
            ));
        }
    }
    let ledger =
        std::fs::read_to_string(data.join("state/runtime-ledger.jsonl")).unwrap_or_default();
    diagnostics.push(format!(
        "ledger tail:\n{}",
        ledger
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    ));
    diagnostics.join("\n")
}

fn captured_command_output(
    stdout_path: &std::path::Path,
    stderr_path: &std::path::Path,
    status: ExitStatus,
) -> Output {
    let stdout = std::fs::read(stdout_path).unwrap_or_default();
    let stderr = std::fs::read(stderr_path).unwrap_or_default();
    let _ = std::fs::remove_file(stdout_path);
    let _ = std::fs::remove_file(stderr_path);
    Output {
        status,
        stdout,
        stderr,
    }
}

fn native_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("native fixture ephemeral port reservation");
    listener
        .local_addr()
        .expect("native fixture local address")
        .port()
}

pub fn tree_snapshot(roots: &[&std::path::Path]) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut snapshot = std::collections::BTreeMap::new();
    for (index, root) in roots.iter().enumerate() {
        let mut files = Vec::new();
        collect_files(root, root, &mut files);
        for (path, bytes) in files {
            snapshot.insert(format!("{index}/{path}"), bytes);
        }
    }
    snapshot
}

fn collect_files(
    root: &std::path::Path,
    path: &std::path::Path,
    files: &mut Vec<(String, Vec<u8>)>,
) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            files.push((relative, std::fs::read(&path).unwrap_or_default()));
        }
    }
}

impl Drop for NativeTerminalFixture {
    fn drop(&mut self) {
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_TEST_TERMINAL_FAULT");
        std::env::remove_var("RPOTATO_TEST_TUI_SECRET_PROBE");
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

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
    }

    impl NativePty {
        pub fn spawn(columns: u16, rows: u16) -> Self {
            let binary = CString::new(env!("CARGO_BIN_EXE_rpotato")).unwrap();
            let tui = CString::new("tui").unwrap();
            let argv = [binary.as_ptr(), tui.as_ptr(), std::ptr::null()];
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

        pub fn wait_for(&mut self, needle: &str) -> String {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                self.drain_available();
                let output = String::from_utf8_lossy(&self.output);
                if output.contains(needle) {
                    return output.into_owned();
                }
                assert!(
                    Instant::now() < deadline,
                    "PTY output timeout; wanted {needle:?}; got {output}"
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
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let mut status = -1;
                // SAFETY: pid belongs to this fixture and status is writable.
                let waited = unsafe { waitpid(self.pid, &mut status, WNOHANG) };
                if waited == self.pid {
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
                let mut status = 0;
                // SAFETY: best-effort termination and reap of the owned child.
                let _ = unsafe { kill(self.pid, 15) };
                // SAFETY: pid still belongs to this fixture until it is reaped.
                let _ = unsafe { waitpid(self.pid, &mut status, 0) };
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

#[cfg(windows)]
mod windows {
    use super::*;
    use std::cell::RefCell;
    use std::ffi::{c_void, OsStr};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::rc::Rc;

    type Bool = i32;
    type Dword = u32;
    type Handle = *mut c_void;
    type HpcOn = Handle;
    type HResult = i32;

    const EXTENDED_STARTUPINFO_PRESENT: Dword = 0x0008_0000;
    const CREATE_UNICODE_ENVIRONMENT: Dword = 0x0000_0400;
    const STARTF_USESTDHANDLES: Dword = 0x0000_0100;
    const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x0002_0016;
    const WAIT_OBJECT_0: Dword = 0;
    const WAIT_TIMEOUT: Dword = 258;
    const INFINITE: Dword = 0xffff_ffff;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Coord {
        x: i16,
        y: i16,
    }

    #[repr(C)]
    struct SecurityAttributes {
        length: Dword,
        security_descriptor: *mut c_void,
        inherit_handle: Bool,
    }

    #[repr(C)]
    struct StartupInfoW {
        cb: Dword,
        reserved: *mut u16,
        desktop: *mut u16,
        title: *mut u16,
        x: Dword,
        y: Dword,
        x_size: Dword,
        y_size: Dword,
        x_count_chars: Dword,
        y_count_chars: Dword,
        fill_attribute: Dword,
        flags: Dword,
        show_window: u16,
        reserved2_bytes: u16,
        reserved2: *mut u8,
        stdin: Handle,
        stdout: Handle,
        stderr: Handle,
    }

    #[repr(C)]
    struct StartupInfoExW {
        startup: StartupInfoW,
        attribute_list: *mut c_void,
    }

    #[repr(C)]
    struct ProcessInformation {
        process: Handle,
        thread: Handle,
        process_id: Dword,
        thread_id: Dword,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreatePipe(
            read_pipe: *mut Handle,
            write_pipe: *mut Handle,
            attributes: *mut SecurityAttributes,
            size: Dword,
        ) -> Bool;
        fn CloseHandle(handle: Handle) -> Bool;
        fn CreatePseudoConsole(
            size: Coord,
            input: Handle,
            output: Handle,
            flags: Dword,
            console: *mut HpcOn,
        ) -> HResult;
        fn ResizePseudoConsole(console: HpcOn, size: Coord) -> HResult;
        fn ClosePseudoConsole(console: HpcOn);
        fn InitializeProcThreadAttributeList(
            list: *mut c_void,
            count: Dword,
            flags: Dword,
            size: *mut usize,
        ) -> Bool;
        fn UpdateProcThreadAttribute(
            list: *mut c_void,
            flags: Dword,
            attribute: usize,
            value: *mut c_void,
            size: usize,
            previous: *mut c_void,
            returned_size: *mut usize,
        ) -> Bool;
        fn DeleteProcThreadAttributeList(list: *mut c_void);
        fn GetProcessHeap() -> Handle;
        fn HeapAlloc(heap: Handle, flags: Dword, bytes: usize) -> *mut c_void;
        fn HeapFree(heap: Handle, flags: Dword, memory: *mut c_void) -> Bool;
        fn CreateProcessW(
            application_name: *const u16,
            command_line: *mut u16,
            process_attributes: *mut c_void,
            thread_attributes: *mut c_void,
            inherit_handles: Bool,
            creation_flags: Dword,
            environment: *mut c_void,
            current_directory: *const u16,
            startup_info: *mut StartupInfoW,
            process_information: *mut ProcessInformation,
        ) -> Bool;
        fn WriteFile(
            handle: Handle,
            buffer: *const c_void,
            bytes: Dword,
            written: *mut Dword,
            overlapped: *mut c_void,
        ) -> Bool;
        fn ReadFile(
            handle: Handle,
            buffer: *mut c_void,
            bytes: Dword,
            read: *mut Dword,
            overlapped: *mut c_void,
        ) -> Bool;
        fn PeekNamedPipe(
            handle: Handle,
            buffer: *mut c_void,
            buffer_size: Dword,
            bytes_read: *mut Dword,
            total_available: *mut Dword,
            bytes_left: *mut Dword,
        ) -> Bool;
        fn WaitForSingleObject(handle: Handle, milliseconds: Dword) -> Dword;
        fn GetExitCodeProcess(process: Handle, exit_code: *mut Dword) -> Bool;
        fn TerminateProcess(process: Handle, exit_code: Dword) -> Bool;
    }

    thread_local! {
        static REUSED_CONSOLE: RefCell<Option<Rc<RefCell<ReusableConsole>>>> =
            const { RefCell::new(None) };
    }

    struct ReusableConsole {
        console: HpcOn,
        input: Handle,
        output: Handle,
        console_input: Handle,
        console_output: Handle,
        probe_binary: PathBuf,
        output_bytes: Vec<u8>,
        active: bool,
    }

    pub struct NativePty {
        session: Rc<RefCell<ReusableConsole>>,
        process: Handle,
        output_start: usize,
        terminal_eof: bool,
        waited: bool,
    }

    pub fn trace_stage(message: &str) {
        eprintln!("[native-terminal] {message}");
        let Some(path) = std::env::var_os("RPOTATO_NATIVE_TERMINAL_TRACE") else {
            return;
        };
        let mut trace = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("native terminal trace file must open");
        writeln!(trace, "[native-terminal] {message}")
            .expect("native terminal trace line must flush");
    }

    impl ReusableConsole {
        fn new(columns: u16, rows: u16) -> Self {
            let mut console_input = std::ptr::null_mut();
            let mut parent_input = std::ptr::null_mut();
            let mut parent_output = std::ptr::null_mut();
            let mut console_output = std::ptr::null_mut();
            // SAFETY: all handle output pointers are valid. The channels are deliberately
            // non-inheritable; the pseudoconsole process attribute owns attachment.
            assert_ne!(
                unsafe {
                    CreatePipe(
                        &mut console_input,
                        &mut parent_input,
                        std::ptr::null_mut(),
                        0,
                    )
                },
                0,
                "ConPTY input pipe creation failed"
            );
            // SAFETY: all handle output pointers are valid.
            assert_ne!(
                unsafe {
                    CreatePipe(
                        &mut parent_output,
                        &mut console_output,
                        std::ptr::null_mut(),
                        0,
                    )
                },
                0,
                "ConPTY output pipe creation failed"
            );

            let mut console = std::ptr::null_mut();
            // SAFETY: pipe ends are valid and console points to writable storage.
            let created = unsafe {
                CreatePseudoConsole(
                    coord(columns, rows),
                    console_input,
                    console_output,
                    0,
                    &mut console,
                )
            };
            assert!(
                created >= 0,
                "CreatePseudoConsole failed: HRESULT={created:#x}"
            );
            let probe_binary = compile_mode_probe();

            Self {
                console,
                input: parent_input,
                output: parent_output,
                console_input,
                console_output,
                probe_binary,
                output_bytes: Vec::new(),
                active: false,
            }
        }

        fn release_creation_pipe_ends(&mut self) {
            // SAFETY: once the first production client has been created, the host-side
            // copies supplied to CreatePseudoConsole are no longer needed.
            unsafe {
                if !self.console_input.is_null() {
                    CloseHandle(self.console_input);
                    self.console_input = std::ptr::null_mut();
                }
                if !self.console_output.is_null() {
                    CloseHandle(self.console_output);
                    self.console_output = std::ptr::null_mut();
                }
            }
        }

        fn run_mode_probe(&mut self) {
            let count_before = mode_probe_values(&self.output_bytes).len();
            let process = launch_in_console(
                self.console,
                &self.probe_binary,
                "",
                &[("RPOTATO_PROBE_EXPECT_ECHO", "1")],
            );
            wait_for_success(process, "terminal mode restoration probe");
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                self.drain_available();
                if mode_probe_values(&self.output_bytes).len() > count_before
                    || Instant::now() >= deadline
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let modes = mode_probe_values(&self.output_bytes);
            assert_eq!(
                modes.len(),
                count_before + 1,
                "same-ConPTY probe must emit exactly one marker per production child"
            );
            assert_eq!(
                modes.last().map(String::as_str),
                Some("1"),
                "same-ConPTY input echo mode was not restored"
            );
        }

        fn drain_available(&mut self) {
            loop {
                let mut available = 0;
                // SAFETY: output is a live pipe handle and available is writable.
                let peeked = unsafe {
                    PeekNamedPipe(
                        self.output,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if peeked == 0 || available == 0 {
                    break;
                }
                let mut buffer = [0u8; 4096];
                let request = available.min(buffer.len() as Dword);
                let mut read_bytes = 0;
                // SAFETY: buffer is writable and request is bounded by its length.
                let read_ok = unsafe {
                    ReadFile(
                        self.output,
                        buffer.as_mut_ptr().cast::<c_void>(),
                        request,
                        &mut read_bytes,
                        std::ptr::null_mut(),
                    )
                };
                if read_ok == 0 || read_bytes == 0 {
                    break;
                }
                self.output_bytes
                    .extend_from_slice(&buffer[..usize::try_from(read_bytes).unwrap()]);
            }
        }
    }

    impl Drop for ReusableConsole {
        fn drop(&mut self) {
            if !self.output.is_null() {
                self.drain_available();
                // SAFETY: closing the host output pipe before ClosePseudoConsole prevents
                // older Windows versions from waiting indefinitely during teardown.
                unsafe { CloseHandle(self.output) };
                self.output = std::ptr::null_mut();
            }
            // SAFETY: the thread-local session owns each remaining live drive handle and HPCON.
            unsafe {
                if !self.input.is_null() {
                    CloseHandle(self.input);
                    self.input = std::ptr::null_mut();
                }
                if !self.console_input.is_null() {
                    CloseHandle(self.console_input);
                    self.console_input = std::ptr::null_mut();
                }
                if !self.console_output.is_null() {
                    CloseHandle(self.console_output);
                    self.console_output = std::ptr::null_mut();
                }
                if !self.console.is_null() {
                    ClosePseudoConsole(self.console);
                    self.console = std::ptr::null_mut();
                }
            }
            let _ = std::fs::remove_file(&self.probe_binary);
        }
    }

    fn reused_console(columns: u16, rows: u16) -> Rc<RefCell<ReusableConsole>> {
        REUSED_CONSOLE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let replace = slot
                .as_ref()
                .is_none_or(|session| session.borrow().input.is_null());
            if replace {
                *slot = Some(Rc::new(RefCell::new(ReusableConsole::new(columns, rows))));
            }
            Rc::clone(slot.as_ref().expect("reused ConPTY session initialized"))
        })
    }

    impl NativePty {
        pub fn spawn(columns: u16, rows: u16) -> Self {
            trace_stage(&format!("spawn {columns}x{rows}"));
            let session = reused_console(columns, rows);
            let (process, output_start) = {
                let mut session_ref = session.borrow_mut();
                assert!(
                    !session_ref.active,
                    "only one child may own the reused ConPTY"
                );
                assert!(
                    !session_ref.input.is_null(),
                    "no production child may launch after ConPTY EOF"
                );
                session_ref.drain_available();
                let output_start = session_ref.output_bytes.len();
                let process = launch_in_console(
                    session_ref.console,
                    std::path::Path::new(env!("CARGO_BIN_EXE_rpotato")),
                    "tui",
                    &[],
                );
                session_ref.release_creation_pipe_ends();
                session_ref.active = true;
                (process, output_start)
            };
            Self {
                session,
                process,
                output_start,
                terminal_eof: false,
                waited: false,
            }
        }

        pub fn resize(&mut self, columns: u16, rows: u16) {
            let console = self.session.borrow().console;
            // SAFETY: console is live and the requested dimensions are positive.
            let result = unsafe { ResizePseudoConsole(console, coord(columns, rows)) };
            assert!(
                result >= 0,
                "ResizePseudoConsole failed: HRESULT={result:#x}"
            );
        }

        pub fn send(&mut self, input: &str) {
            let handle = self.session.borrow().input;
            assert!(!handle.is_null(), "ConPTY input is closed");
            let input = input.replace("\r\n", "\n").replace('\n', "\r");
            let mut offset = 0usize;
            while offset < input.len() {
                let remaining = &input.as_bytes()[offset..];
                let request = Dword::try_from(remaining.len()).unwrap_or(Dword::MAX);
                let mut written = 0;
                // SAFETY: input is a live pipe handle and the byte slice is readable.
                assert_ne!(
                    unsafe {
                        WriteFile(
                            handle,
                            remaining.as_ptr().cast::<c_void>(),
                            request,
                            &mut written,
                            std::ptr::null_mut(),
                        )
                    },
                    0,
                    "ConPTY input write failed"
                );
                assert!(written > 0);
                offset += usize::try_from(written).unwrap();
            }
        }

        pub fn send_eof(&mut self) {
            // Windows console line input represents EOF as Ctrl+Z followed by Enter.
            // The stream cannot host another probe after EOF, so finish closes it.
            self.terminal_eof = true;
            self.send("\u{001a}\n");
        }

        pub fn wait_for(&mut self, needle: &str) -> String {
            trace_stage(&format!("wait for {needle:?}"));
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let output = {
                    let mut session = self.session.borrow_mut();
                    session.drain_available();
                    String::from_utf8_lossy(&session.output_bytes[self.output_start..]).into_owned()
                };
                if output.contains(needle) {
                    trace_stage(&format!("found {needle:?}"));
                    return output;
                }
                assert!(
                    Instant::now() < deadline,
                    "ConPTY output timeout; wanted {needle:?}; got {output}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        pub fn finish(self) -> String {
            self.finish_with_status(true)
        }

        pub fn finish_failure(self) -> String {
            self.finish_with_status(false)
        }

        fn finish_with_status(mut self, success: bool) -> String {
            trace_stage(&format!("wait for child; success={success}"));
            // SAFETY: process is a live child handle.
            let wait = unsafe { WaitForSingleObject(self.process, 10_000) };
            assert_eq!(wait, WAIT_OBJECT_0, "ConPTY child wait failed: {wait}");
            let mut exit_code = Dword::MAX;
            // SAFETY: exit_code is writable and the process handle is valid.
            assert_ne!(
                unsafe { GetExitCodeProcess(self.process, &mut exit_code) },
                0
            );
            if success {
                assert_eq!(exit_code, 0, "ConPTY child failed");
            } else {
                assert_ne!(exit_code, 0, "ConPTY child unexpectedly succeeded");
            }
            trace_stage(&format!("child exited with {exit_code:#x}"));
            let output = {
                let mut session = self.session.borrow_mut();
                session.drain_available();
                if self.terminal_eof {
                    // SAFETY: EOF is terminal for this reused ConPTY input stream.
                    unsafe { CloseHandle(session.input) };
                    session.input = std::ptr::null_mut();
                } else {
                    trace_stage("run echo restoration probe");
                    session.run_mode_probe();
                    trace_stage("echo restoration probe passed");
                }
                session.active = false;
                String::from_utf8_lossy(&session.output_bytes[self.output_start..]).into_owned()
            };
            self.waited = true;
            output
        }
    }

    impl Drop for NativePty {
        fn drop(&mut self) {
            if !self.waited && !self.process.is_null() {
                // SAFETY: best-effort termination of the owned child.
                unsafe {
                    TerminateProcess(self.process, 1);
                    WaitForSingleObject(self.process, INFINITE);
                }
            }
            if !self.process.is_null() {
                // SAFETY: the fixture owns this production child handle exactly once.
                unsafe { CloseHandle(self.process) };
                self.process = std::ptr::null_mut();
            }
            self.session.borrow_mut().active = false;
        }
    }

    fn compile_mode_probe() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let stem = std::env::temp_dir().join(format!(
            "rpotato-native-mode-probe-{}-{nonce}",
            std::process::id()
        ));
        let binary = stem.with_extension("exe");
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/support/platform/native_terminal_probe.rs");
        let output = Command::new("rustc")
            .arg("--edition=2021")
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .expect("rustc mode probe launch failed");
        assert!(
            output.status.success(),
            "rustc mode probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        binary
    }

    fn launch_in_console(
        console: HpcOn,
        application: &std::path::Path,
        arguments: &str,
        environment_overrides: &[(&str, &str)],
    ) -> Handle {
        let heap = unsafe { GetProcessHeap() };
        let mut attribute_bytes = 0usize;
        unsafe {
            InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut attribute_bytes);
        }
        assert!(
            attribute_bytes > 0,
            "ConPTY attribute size discovery failed"
        );
        let attribute_list = unsafe { HeapAlloc(heap, 0, attribute_bytes) };
        assert!(
            !attribute_list.is_null(),
            "ConPTY attribute allocation failed"
        );
        assert_ne!(
            unsafe {
                InitializeProcThreadAttributeList(attribute_list, 1, 0, &mut attribute_bytes)
            },
            0,
            "ConPTY attribute initialization failed"
        );
        assert_ne!(
            unsafe {
                UpdateProcThreadAttribute(
                    attribute_list,
                    0,
                    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                    console,
                    std::mem::size_of::<HpcOn>(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0,
            "ConPTY attribute update failed"
        );
        let mut startup: StartupInfoExW = unsafe { std::mem::zeroed() };
        startup.startup.cb = std::mem::size_of::<StartupInfoExW>() as Dword;
        // Cargo's redirected test host exposes inherited standard handles. Mark the
        // deliberately zeroed fields as authoritative so only the ConPTY attribute
        // supplies the child's console handles.
        startup.startup.flags = STARTF_USESTDHANDLES;
        startup.attribute_list = attribute_list;
        let mut process: ProcessInformation = unsafe { std::mem::zeroed() };
        let command_text = if arguments.is_empty() {
            format!("\"{}\"", application.display())
        } else {
            format!("\"{}\" {arguments}", application.display())
        };
        let mut command = wide(OsStr::new(&command_text));
        let mut environment = explicit_environment_block(environment_overrides);
        let launched = unsafe {
            CreateProcessW(
                std::ptr::null(),
                command.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                environment.as_mut_ptr().cast::<c_void>(),
                std::ptr::null(),
                &mut startup.startup,
                &mut process,
            )
        };
        unsafe {
            DeleteProcThreadAttributeList(attribute_list);
            HeapFree(heap, 0, attribute_list);
        }
        assert_ne!(launched, 0, "ConPTY child creation failed");
        unsafe { CloseHandle(process.thread) };
        process.process
    }

    fn explicit_environment_block(overrides: &[(&str, &str)]) -> Vec<u16> {
        let mut entries = std::env::vars_os()
            .map(|(key, value)| format!("{}={}", key.to_string_lossy(), value.to_string_lossy()))
            .collect::<Vec<_>>();
        for (key, value) in overrides {
            entries.retain(|entry| {
                entry
                    .split_once('=')
                    .is_none_or(|(existing, _)| !existing.eq_ignore_ascii_case(key))
            });
            entries.push(format!("{key}={value}"));
        }
        entries.sort_by_key(|entry| entry.to_ascii_uppercase());
        let mut block = Vec::new();
        for entry in entries {
            block.extend(OsStr::new(&entry).encode_wide());
            block.push(0);
        }
        block.push(0);
        block
    }

    fn wait_for_success(process: Handle, context: &str) {
        let wait = unsafe { WaitForSingleObject(process, 10_000) };
        assert_eq!(wait, WAIT_OBJECT_0, "{context} wait failed: {wait}");
        let mut exit_code = Dword::MAX;
        assert_ne!(unsafe { GetExitCodeProcess(process, &mut exit_code) }, 0);
        assert_eq!(exit_code, 0, "{context} failed");
        unsafe { CloseHandle(process) };
    }

    fn mode_probe_values(output: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(output)
            .lines()
            .filter_map(|line| line.trim().strip_prefix("MODE ECHO="))
            .map(str::to_string)
            .collect()
    }

    fn coord(columns: u16, rows: u16) -> Coord {
        Coord {
            x: i16::try_from(columns).expect("ConPTY columns fit i16"),
            y: i16::try_from(rows).expect("ConPTY rows fit i16"),
        }
    }

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    #[allow(dead_code)]
    const _: Dword = WAIT_TIMEOUT;
}

#[cfg(windows)]
pub use windows::{trace_stage, NativePty};
