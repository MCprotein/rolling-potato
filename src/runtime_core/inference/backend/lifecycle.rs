use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendSidecarRecord {
    pub(crate) backend_id: String,
    pub(crate) pid: u32,
    pub(crate) binary_path: PathBuf,
    pub(crate) model_path: PathBuf,
    pub(crate) model_sha256: String,
    pub(crate) model_size_bytes: u64,
    pub(crate) backend_release: String,
    pub(crate) binary_sha256: String,
    pub(crate) mmproj: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) ctx_size: Option<u32>,
    pub(crate) stdout_log: PathBuf,
    pub(crate) stderr_log: PathBuf,
    pub(crate) started_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendGenerationRecord {
    pub(crate) generation_id: String,
    pub(crate) client_pid: u32,
    pub(crate) sidecar_pid: u32,
    pub(crate) started_at_ms: u128,
    pub(crate) timeout_ms: u32,
    pub(crate) streaming_display: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackendGenerationTerminalRecord {
    pub(crate) generation_id: String,
    pub(crate) outcome: String,
    pub(crate) lifecycle_event: String,
    pub(crate) recorded_at_ms: u128,
}

pub(crate) fn render_generation_record(record: &BackendGenerationRecord) -> String {
    format!(
        "generation_id={}\nclient_pid={}\nsidecar_pid={}\nstarted_at_ms={}\ntimeout_ms={}\nstreaming_display={}\n",
        record.generation_id,
        record.client_pid,
        record.sidecar_pid,
        record.started_at_ms,
        record.timeout_ms,
        record.streaming_display
    )
}

pub(crate) fn parse_generation_record(contents: &str) -> Option<BackendGenerationRecord> {
    Some(BackendGenerationRecord {
        generation_id: record_value(contents, "generation_id")?.to_string(),
        client_pid: record_value(contents, "client_pid")?.parse().ok()?,
        sidecar_pid: record_value(contents, "sidecar_pid")?.parse().ok()?,
        started_at_ms: record_value(contents, "started_at_ms")?.parse().ok()?,
        timeout_ms: record_value(contents, "timeout_ms")?.parse().ok()?,
        streaming_display: record_value(contents, "streaming_display")?.parse().ok()?,
    })
}

pub(crate) fn render_sidecar_record(record: &BackendSidecarRecord) -> String {
    format!(
        "backend_id={}\npid={}\nbinary_path={}\nmodel_path={}\nmodel_sha256={}\nmodel_size_bytes={}\nbackend_release={}\nbinary_sha256={}\nmmproj={}\nhost={}\nport={}\nctx_size={}\nstdout_log={}\nstderr_log={}\nstarted_at_ms={}\n",
        record.backend_id,
        record.pid,
        record.binary_path.display(),
        record.model_path.display(),
        record.model_sha256,
        record.model_size_bytes,
        record.backend_release,
        record.binary_sha256,
        record.mmproj,
        record.host,
        record.port,
        record
            .ctx_size
            .map(|value| value.to_string())
            .unwrap_or_default(),
        record.stdout_log.display(),
        record.stderr_log.display(),
        record.started_at_ms
    )
}

pub(crate) fn parse_sidecar_record(contents: &str) -> Option<BackendSidecarRecord> {
    let mut backend_id = None;
    let mut pid = None;
    let mut binary_path = None;
    let mut model_path = None;
    let mut model_sha256 = None;
    let mut model_size_bytes = None;
    let mut backend_release = None;
    let mut binary_sha256 = None;
    let mut mmproj = None;
    let mut host = None;
    let mut port = None;
    let mut ctx_size = None;
    let mut stdout_log = None;
    let mut stderr_log = None;
    let mut started_at_ms = None;

    for line in contents.lines() {
        let (key, value) = line.split_once('=')?;
        match key {
            "backend_id" => backend_id = Some(value.to_string()),
            "pid" => pid = value.parse::<u32>().ok(),
            "binary_path" => binary_path = Some(PathBuf::from(value)),
            "model_path" => model_path = Some(PathBuf::from(value)),
            "model_sha256" => model_sha256 = Some(value.to_string()),
            "model_size_bytes" => model_size_bytes = value.parse::<u64>().ok(),
            "backend_release" => backend_release = Some(value.to_string()),
            "binary_sha256" => binary_sha256 = Some(value.to_string()),
            "mmproj" => mmproj = Some(value.to_string()),
            "host" => host = Some(value.to_string()),
            "port" => port = value.parse::<u16>().ok(),
            "ctx_size" => {
                ctx_size = if value.is_empty() || value == "model-default" {
                    Some(None)
                } else {
                    let parsed = value.parse::<u32>().ok()?;
                    if parsed == 0 {
                        return None;
                    }
                    Some(Some(parsed))
                };
            }
            "stdout_log" => stdout_log = Some(PathBuf::from(value)),
            "stderr_log" => stderr_log = Some(PathBuf::from(value)),
            "started_at_ms" => started_at_ms = value.parse::<u128>().ok(),
            _ => {}
        }
    }

    Some(BackendSidecarRecord {
        backend_id: backend_id?,
        pid: pid?,
        binary_path: binary_path?,
        model_path: model_path?,
        model_sha256: model_sha256.unwrap_or_else(|| "unknown".to_string()),
        model_size_bytes: model_size_bytes.unwrap_or(0),
        backend_release: backend_release.unwrap_or_else(|| "unknown".to_string()),
        binary_sha256: binary_sha256.unwrap_or_else(|| "unknown".to_string()),
        mmproj: mmproj.unwrap_or_else(|| "unknown".to_string()),
        host: host?,
        port: port?,
        ctx_size: ctx_size.unwrap_or(None),
        stdout_log: stdout_log?,
        stderr_log: stderr_log?,
        started_at_ms: started_at_ms?,
    })
}

pub(crate) fn record_value<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    contents.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate == key).then_some(value)
    })
}
