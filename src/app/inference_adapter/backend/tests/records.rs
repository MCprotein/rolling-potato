#[test]
fn backend_status_reports_stopped_without_record() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-backend-status-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));

    let report = status_report().unwrap();
    let snapshot = runtime_snapshot().unwrap();

    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    fs::remove_dir_all(root).unwrap();
    assert!(report.contains("status: stopped"));
    assert_eq!(snapshot.status, "stopped");
    assert_eq!(snapshot.model_id, None);
    assert_eq!(snapshot.model_path, None);
    assert_eq!(snapshot.context_limit_tokens, None);
}

#[test]
fn sidecar_record_round_trip_preserves_ctx_size_and_vision_artifact() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-backend-record-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));

    let record = BackendSidecarRecord {
        backend_id: LLAMA_CPP_BACKEND_ID.to_string(),
        pid: 1234,
        binary_path: root.join("llama-server"),
        model_path: root.join("model.gguf"),
        model_sha256: "a".repeat(64),
        model_size_bytes: 1024,
        backend_release: LLAMA_CPP_RELEASE.release_tag.to_string(),
        binary_sha256: "b".repeat(64),
        mmproj: "required".to_string(),
        mmproj_path: Some(root.join("mmproj.gguf")),
        mmproj_sha256: Some("c".repeat(64)),
        mmproj_size_bytes: Some(512),
        host: DEFAULT_HOST.to_string(),
        port: DEFAULT_PORT,
        ctx_size: Some(4096),
        stdout_log: root.join("stdout.log"),
        stderr_log: root.join("stderr.log"),
        started_at_ms: now_ms(),
    };
    backend_state::write_sidecar_record(&record).unwrap();
    let expected = format!(
        "backend_id={}\npid={}\nbinary_path={}\nmodel_path={}\nmodel_sha256={}\nmodel_size_bytes={}\nbackend_release={}\nbinary_sha256={}\nmmproj={}\nmmproj_path={}\nmmproj_sha256={}\nmmproj_size_bytes={}\nhost={}\nport={}\nctx_size={}\nstdout_log={}\nstderr_log={}\nstarted_at_ms={}\n",
        record.backend_id,
        record.pid,
        record.binary_path.display(),
        record.model_path.display(),
        record.model_sha256,
        record.model_size_bytes,
        record.backend_release,
        record.binary_sha256,
        record.mmproj,
        record.mmproj_path.as_ref().unwrap().display(),
        record.mmproj_sha256.as_ref().unwrap(),
        record.mmproj_size_bytes.unwrap(),
        record.host,
        record.port,
        record.ctx_size.unwrap(),
        record.stdout_log.display(),
        record.stderr_log.display(),
        record.started_at_ms
    );
    assert_eq!(
        fs::read_to_string(backend_state::sidecar_record_path()).unwrap(),
        expected
    );
    let restored = backend_state::read_sidecar_record().unwrap().unwrap();

    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    fs::remove_dir_all(root).unwrap();

    assert_eq!(restored.ctx_size, Some(4096));
    assert_eq!(restored.mmproj_path, record.mmproj_path);
    assert_eq!(restored.mmproj_sha256, record.mmproj_sha256);
    assert_eq!(restored.mmproj_size_bytes, Some(512));
}

#[test]
fn generation_record_codec_preserves_exact_bytes_and_round_trips() {
    let record = BackendGenerationRecord {
        generation_id: "generation-codec".to_string(),
        client_pid: 101,
        sidecar_pid: 202,
        started_at_ms: 303,
        timeout_ms: 404,
        streaming_display: true,
    };

    let rendered = render_generation_record(&record);

    assert_eq!(
        rendered,
        "generation_id=generation-codec\nclient_pid=101\nsidecar_pid=202\nstarted_at_ms=303\ntimeout_ms=404\nstreaming_display=true\n"
    );
    assert_eq!(parse_generation_record(&rendered), Some(record));
}
