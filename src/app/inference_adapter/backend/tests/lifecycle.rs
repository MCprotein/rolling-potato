#[cfg(unix)]
#[test]
fn stop_removes_stale_sidecar_record() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-backend-lifecycle-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));

    let model_path = root.join("model.gguf");
    fs::write(&model_path, b"fake model").unwrap();
    let record = BackendSidecarRecord {
        backend_id: LLAMA_CPP_BACKEND_ID.to_string(),
        pid: u32::MAX,
        binary_path: fs::canonicalize("/bin/sleep").unwrap(),
        model_path: fs::canonicalize(&model_path).unwrap(),
        model_sha256: checksum::sha256_file(&model_path).unwrap(),
        model_size_bytes: 10,
        backend_release: LLAMA_CPP_RELEASE.release_tag.to_string(),
        binary_sha256: checksum::sha256_file(Path::new("/bin/sleep")).unwrap(),
        mmproj: "not-required-text-only".to_string(),
        mmproj_path: None,
        mmproj_sha256: None,
        mmproj_size_bytes: None,
        host: DEFAULT_HOST.to_string(),
        port: 65534,
        ctx_size: Some(4096),
        stdout_log: root.join("stdout.log"),
        stderr_log: root.join("stderr.log"),
        started_at_ms: now_ms(),
    };
    backend_state::write_sidecar_record(&record).unwrap();

    let status = status_report().unwrap();
    let stop = stop_report().unwrap();
    let record_after_stop = backend_state::read_sidecar_record().unwrap();

    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    env::remove_var(ENV_BACKEND_PORT);
    let _ = fs::remove_dir_all(root);

    assert!(status.contains("status: stale"));
    assert!(stop.contains("status: stale-record-removed"));
    assert!(record_after_stop.is_none());
}

#[cfg(unix)]
#[test]
fn start_timeout_removes_record_and_keeps_logs() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-backend-timeout-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    env::set_var(ENV_BACKEND_PORT, "65534");

    let backend_script = root.join("fake-llama-server-timeout");
    fs::write(
        &backend_script,
        "#!/bin/sh\necho 'booting stdout'\necho 'booting stderr' >&2\nexec sleep 10\n",
    )
    .unwrap();
    llama_install::set_executable_bit(&backend_script).unwrap();
    env::set_var(ENV_BACKEND_PATH, &backend_script);

    let model_path = root.join("model.gguf");
    fs::write(&model_path, b"fake model").unwrap();
    let err = start_sidecar_with_timeout(
        model_path.to_str().unwrap(),
        Some(4096),
        Duration::from_millis(200),
    )
    .unwrap_err();
    let stdout_logs = fs::read_dir(paths::logs_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains("stdout"))
        .count();
    let stderr_logs = fs::read_dir(paths::logs_dir())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains("stderr"))
        .count();
    let record = backend_state::read_sidecar_record().unwrap();

    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    env::remove_var(ENV_BACKEND_PATH);
    env::remove_var(ENV_BACKEND_PORT);
    let _ = fs::remove_dir_all(root);

    assert_eq!(err.code, 3);
    assert!(err.message.contains("backend start timeout"));
    assert!(record.is_none());
    assert!(stdout_logs > 0);
    assert!(stderr_logs > 0);
}
