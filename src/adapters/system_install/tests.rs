use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

include!("tests/profile_path.rs");
include!("tests/clean_state.rs");
include!("tests/binary_update.rs");
include!("tests/uninstall.rs");

fn unique_temp(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    env::temp_dir().join(format!(
        "rpotato-system-install-{label}-{}-{nonce}",
        std::process::id()
    ))
}
