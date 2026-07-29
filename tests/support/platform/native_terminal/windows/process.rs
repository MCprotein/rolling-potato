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
        unsafe { InitializeProcThreadAttributeList(attribute_list, 1, 0, &mut attribute_bytes) },
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
