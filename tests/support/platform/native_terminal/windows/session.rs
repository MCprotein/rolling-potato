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
