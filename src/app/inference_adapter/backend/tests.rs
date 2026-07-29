use super::*;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

fn generation_test_sidecar() -> BackendSidecarRecord {
    BackendSidecarRecord {
        backend_id: LLAMA_CPP_BACKEND_ID.to_string(),
        pid: std::process::id(),
        binary_path: PathBuf::from("llama-server"),
        model_path: PathBuf::from("model.gguf"),
        model_sha256: "a".repeat(64),
        model_size_bytes: 1,
        backend_release: LLAMA_CPP_RELEASE.release_tag.to_string(),
        binary_sha256: "b".repeat(64),
        mmproj: "not-required-text-only".to_string(),
        mmproj_path: None,
        mmproj_sha256: None,
        mmproj_size_bytes: None,
        host: DEFAULT_HOST.to_string(),
        port: DEFAULT_PORT,
        ctx_size: Some(4096),
        stdout_log: PathBuf::from("stdout.log"),
        stderr_log: PathBuf::from("stderr.log"),
        started_at_ms: now_ms(),
    }
}

fn write_test_tar_gz(path: &Path, files: &[(&str, &[u8])]) -> std::io::Result<()> {
    let file = File::create(path)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (file_path, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_path(file_path)?;
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, *bytes)?;
    }
    let encoder = builder.into_inner()?;
    encoder.finish()?;
    Ok(())
}

include!("tests/termination.rs");
include!("tests/discovery.rs");
include!("tests/installation.rs");
include!("tests/records.rs");
include!("tests/generation.rs");
include!("tests/lifecycle.rs");
include!("tests/diagnostics.rs");
