//! llama-server sidecar command construction.

use std::path::Path;
use std::process::Command;

pub(crate) fn sidecar_command(
    binary_path: &Path,
    model_path: &Path,
    mmproj_path: Option<&Path>,
    host: &str,
    port: u16,
    ctx_size: Option<u32>,
) -> Command {
    let mut command = Command::new(binary_path);
    command
        .arg("--model")
        .arg(model_path)
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(port.to_string());
    if let Some(mmproj_path) = mmproj_path {
        command.arg("--mmproj").arg(mmproj_path);
    }
    if let Some(ctx_size) = ctx_size {
        command.arg("--ctx-size").arg(ctx_size.to_string());
    }
    command
}
