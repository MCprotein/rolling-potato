pub struct NativePty {
    session: Rc<RefCell<ReusableConsole>>,
    process: Handle,
    output_start: usize,
    terminal_eof: bool,
    waited: bool,
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
