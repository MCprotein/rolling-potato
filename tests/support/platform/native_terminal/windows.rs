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
                    "",
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
pub use windows::NativePty;
