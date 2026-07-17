#![cfg(windows)]

use std::ffi::c_void;

type Handle = *mut c_void;

const STD_INPUT_HANDLE: u32 = (-10_i32) as u32;
const ENABLE_ECHO_INPUT: u32 = 0x0004;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(which: u32) -> Handle;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
}

fn main() {
    let expected = match std::env::var("RPOTATO_PROBE_EXPECT_ECHO").as_deref() {
        Ok("0") => false,
        Ok("1") => true,
        _ => std::process::exit(22),
    };
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if handle.is_null() || handle as isize == -1 {
        std::process::exit(20);
    }
    let mut mode = 0_u32;
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        std::process::exit(21);
    }
    let echo = mode & ENABLE_ECHO_INPUT != 0;
    println!("MODE ECHO={}", u8::from(echo));
    if echo != expected {
        std::process::exit(22);
    }
}
