use std::path::PathBuf;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{fs::OpenOptions, io::Write};

static NATIVE_TERMINAL_LOCK: Mutex<()> = Mutex::new(());
static SOURCE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
#[cfg(not(windows))]
const FIXTURE_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(windows)]
const FIXTURE_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

#[path = "native_terminal/capture.rs"]
mod capture;
#[path = "native_terminal/fixture.rs"]
mod fixture;
#[path = "native_terminal/process.rs"]
mod process;
#[path = "native_terminal/trace.rs"]
mod trace;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "native_terminal/unix.rs"]
mod unix;
#[cfg(windows)]
#[path = "native_terminal/windows.rs"]
mod windows;

pub use capture::tree_snapshot;
pub(crate) use capture::{mode_probe_values, strip_terminal_controls};
pub use fixture::{NativeTerminalFixture, PendingSourceApproval};
use process::{backend_failure_diagnostics, native_port, run_bounded_command};
pub(crate) use trace::trace_stage;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use unix::NativePty;
#[cfg(windows)]
pub use windows::NativePty;
