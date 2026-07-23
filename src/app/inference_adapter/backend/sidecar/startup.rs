use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use crate::adapters::filesystem::runtime_mutation;

use super::*;

pub(in crate::app::inference_adapter::backend) fn start_sidecar_with_timeout(
    model_path: &str,
    ctx_size: Option<u32>,
    timeout: Duration,
) -> Result<String, AppError> {
    let runtime_transition = runtime_mutation::acquire("backend start")?;
    let model_path = canonical_existing_file(model_path, "model")?;
    let discovery = llama_backend::discover();
    if !discovery.binary_exists || !discovery.binary_is_file {
        return Err(AppError::blocked(format!(
            "backend start 차단\n- 이유: backend binary를 찾지 못했습니다.\n- selected binary: {}\n- 다음 단계: rpotato backend install 또는 {} 설정",
            discovery.selected_path.display(),
            ENV_BACKEND_PATH
        )));
    }
    if !discovery.binary_executable {
        return Err(AppError::blocked(format!(
            "backend start 차단\n- 이유: backend binary 실행 권한이 없습니다.\n- selected binary: {}",
            discovery.selected_path.display()
        )));
    }

    if let Some(record) = backend_state::read_sidecar_record()? {
        if backend_process::is_running(record.pid) {
            let resource_sample = record_backend_resource_sample(&record, "start-existing")?;
            return Ok(format!(
                "backend start\n- status: already-running\n- pid: {}\n- binary: {}\n- model: {}\n- vision: {}\n- mmproj: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- stdout log: {}\n- stderr log: {}",
                record.pid,
                record.binary_path.display(),
                record.model_path.display(),
                if record.mmproj_path.is_some() {
                    "ready"
                } else {
                    "unavailable (text-ready)"
                },
                record
                    .mmproj_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "없음".to_string()),
                record.host,
                record.port,
                display_optional_u32(record.ctx_size),
                resource_sample.metric.pressure_status,
                display_optional_f64(resource_sample.metric.process_cpu_percent),
                display_optional_u64_unknown(resource_sample.metric.average_rss_bytes),
                display_optional_u64_unknown(resource_sample.metric.peak_rss_bytes),
                display_optional_u64_unknown(resource_sample.metric.disk_bytes),
                resource_sample.ledger_event,
                record.stdout_log.display(),
                record.stderr_log.display()
            ));
        }
        backend_state::remove_sidecar_record()?;
    }

    let binary_path = fs::canonicalize(&discovery.selected_path).map_err(|err| {
        AppError::runtime(format!(
            "backend binary canonical path 계산 실패: {} ({err})",
            discovery.selected_path.display()
        ))
    })?;
    let model_size_bytes = model_path
        .metadata()
        .map_err(|err| {
            AppError::runtime(format!(
                "model artifact metadata를 읽지 못했습니다: {} ({err})",
                model_path.display()
            ))
        })?
        .len();
    let model_sha256 = checksum::sha256_file(&model_path)?;
    let vision_projector =
        crate::app::inference_adapter::model::verified_vision_projector(&model_path, &model_sha256);
    let binary_sha256 = checksum::sha256_file(&binary_path)?;
    let backend_release = if discovery.selected_source == "managed" {
        LLAMA_CPP_RELEASE.release_tag.to_string()
    } else {
        "override-unverified".to_string()
    };
    fs::create_dir_all(paths::logs_dir()).map_err(|err| {
        AppError::runtime(format!(
            "backend log directory를 만들지 못했습니다: {} ({err})",
            paths::logs_dir().display()
        ))
    })?;
    let run_id = now_ms();
    let stdout_log = paths::logs_dir().join(format!("backend-llama.cpp-{run_id}-stdout.log"));
    let stderr_log = paths::logs_dir().join(format!("backend-llama.cpp-{run_id}-stderr.log"));
    let stdout_file = backend_state::create_log_file(&stdout_log)?;
    let stderr_file = backend_state::create_log_file(&stderr_log)?;
    trace_backend_start("logs-created");

    let mut command = llama_backend::sidecar_command(
        &binary_path,
        &model_path,
        vision_projector
            .as_ref()
            .map(|projector| projector.path.as_path()),
        discovery.host,
        discovery.port,
        ctx_size,
    );
    backend_process::configure_child(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|err| {
            AppError::runtime(format!(
                "backend sidecar 시작 실패: {} ({err})",
                binary_path.display()
            ))
        })?;
    trace_backend_start(&format!("sidecar-spawned pid={}", child.id()));

    let record = BackendSidecarRecord {
        backend_id: discovery.adapter_id.to_string(),
        pid: child.id(),
        binary_path,
        model_path,
        model_sha256,
        model_size_bytes,
        backend_release,
        binary_sha256,
        mmproj: if vision_projector.is_some() {
            "required".to_string()
        } else {
            "not-required-text-only".to_string()
        },
        mmproj_path: vision_projector
            .as_ref()
            .map(|projector| projector.path.clone()),
        mmproj_sha256: vision_projector
            .as_ref()
            .map(|projector| projector.sha256.clone()),
        mmproj_size_bytes: vision_projector
            .as_ref()
            .map(|projector| projector.size_bytes),
        host: discovery.host.to_string(),
        port: discovery.port,
        ctx_size,
        stdout_log,
        stderr_log,
        started_at_ms: now_ms(),
    };
    if let Err(err) = backend_state::write_sidecar_record(&record) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }
    trace_backend_start("sidecar-record-written");
    drop(runtime_transition);

    let started_at = Instant::now();
    loop {
        trace_backend_start("health-probe-start");
        let health = llama_backend::probe_health(
            &record.host,
            record.port,
            Duration::from_millis(HEALTH_TIMEOUT_MS),
        );
        trace_backend_start(&format!("health-probe-finished status={}", health.status));
        if health.status == "healthy" {
            let startup_ms = started_at.elapsed().as_millis();
            let event_id = state::record_event(
                "backend.sidecar.start.completed",
                "backend sidecar 시작 완료",
                &format!(
                    "pid={} backend={} backend_release={} binary_sha256={} model_id={} model_sha256={} model_size_bytes={} port={} ctx_size={} mmproj={} sampling=temperature-0.1_top-p-0.8 host_os={} host_arch={} startup_ms={}",
                    record.pid,
                    record.backend_id,
                    record.backend_release,
                    record.binary_sha256,
                    model_id_from_path(&record.model_path),
                    record.model_sha256,
                    record.model_size_bytes,
                    record.port,
                    display_optional_u32(record.ctx_size),
                    record.mmproj,
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    startup_ms
                ),
            )?;
            trace_backend_start("start-event-recorded");
            let resource_sample = record_backend_resource_sample(&record, "start")?;
            trace_backend_start("resource-sample-recorded");
            return Ok(format!(
                "backend start\n- status: running\n- pid: {}\n- binary: {}\n- model: {}\n- vision: {}\n- mmproj: {}\n- host: {}\n- port: {}\n- ctx size: {}\n- startup ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- resource sample event: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
                record.pid,
                record.binary_path.display(),
                record.model_path.display(),
                if record.mmproj_path.is_some() {
                    "ready"
                } else {
                    "unavailable (text-ready)"
                },
                record
                    .mmproj_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "없음".to_string()),
                record.host,
                record.port,
                display_optional_u32(record.ctx_size),
                startup_ms,
                resource_sample.metric.pressure_status,
                display_optional_f64(resource_sample.metric.process_cpu_percent),
                display_optional_u64_unknown(resource_sample.metric.average_rss_bytes),
                display_optional_u64_unknown(resource_sample.metric.peak_rss_bytes),
                display_optional_u64_unknown(resource_sample.metric.disk_bytes),
                resource_sample.ledger_event,
                record.stdout_log.display(),
                record.stderr_log.display(),
                event_id
            ));
        }

        if let Some(status) = child.try_wait().map_err(|err| {
            AppError::runtime(format!("backend sidecar process 상태 확인 실패: {err}"))
        })? {
            backend_state::remove_sidecar_record()?;
            let event_id = state::record_event(
                "backend.sidecar.start.failed",
                "backend sidecar 시작 실패",
                &format!(
                    "pid={} exit_status={} stdout_log={} stderr_log={}",
                    record.pid,
                    status,
                    record.stdout_log.display(),
                    record.stderr_log.display()
                ),
            )?;
            return Err(AppError::blocked(format!(
                "backend start 실패\n- pid: {}\n- exit status: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
                record.pid,
                status,
                record.stdout_log.display(),
                record.stderr_log.display(),
                event_id
            )));
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            backend_state::remove_sidecar_record()?;
            let event_id = state::record_event(
                "backend.sidecar.start.timeout",
                "backend sidecar 시작 timeout",
                &format!(
                    "pid={} timeout_ms={} stdout_log={} stderr_log={}",
                    record.pid,
                    timeout.as_millis(),
                    record.stdout_log.display(),
                    record.stderr_log.display()
                ),
            )?;
            return Err(AppError::blocked(format!(
                "backend start timeout\n- pid: {}\n- timeout ms: {}\n- stdout log: {}\n- stderr log: {}\n- ledger event: {}",
                record.pid,
                timeout.as_millis(),
                record.stdout_log.display(),
                record.stderr_log.display(),
                event_id
            )));
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn canonical_existing_file(path: &str, label: &str) -> Result<PathBuf, AppError> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(AppError::usage(format!(
            "{label} file을 찾지 못했습니다: {}",
            path.display()
        )));
    }
    fs::canonicalize(&path).map_err(|err| {
        AppError::runtime(format!(
            "{label} file canonical path 계산 실패: {} ({err})",
            path.display()
        ))
    })
}
