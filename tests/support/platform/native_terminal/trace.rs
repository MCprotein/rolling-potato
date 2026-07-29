use super::*;

pub(crate) fn trace_stage(message: &str) {
    eprintln!("[native-terminal] {message}");
    let Some(path) = std::env::var_os("RPOTATO_NATIVE_TERMINAL_TRACE") else {
        return;
    };
    let Ok(mut trace) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(trace, "[native-terminal] {message}");
}
