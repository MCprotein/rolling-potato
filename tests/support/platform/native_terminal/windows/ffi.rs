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

#[allow(dead_code)]
const _: Dword = WAIT_TIMEOUT;
