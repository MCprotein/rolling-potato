#[test]
fn runtime_mutation_lease_excludes_backend_and_generation_publish() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let _clean_transition =
        crate::adapters::filesystem::runtime_mutation::acquire("clean transition test").unwrap();

    let generation_err =
        begin_active_generation(&generation_test_sidecar(), 1_000, false).unwrap_err();
    let backend_err =
        start_sidecar_with_timeout("missing-model.gguf", Some(128), Duration::from_millis(1))
            .unwrap_err();

    assert_eq!(generation_err.code, 3);
    assert!(generation_err
        .message
        .contains("backend generation begin lock"));
    assert_eq!(backend_err.code, 3);
    assert!(backend_err.message.contains("backend start lock"));
}

#[test]
fn generation_start_does_not_delete_foreign_cancel_marker() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-generation-marker-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    fs::create_dir_all(paths::state_dir()).unwrap();
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &backend_state::generation_cancel_path(),
        b"generation_id=another-generation\n",
    )
    .unwrap();
    let sidecar = BackendSidecarRecord {
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
    };

    let generation = begin_active_generation(&sidecar, 1_000, false).unwrap();
    let marker = fs::read_to_string(backend_state::generation_cancel_path()).unwrap();

    assert!(marker.contains("generation_id=another-generation"));
    release_generation_admission(&generation.generation_id).unwrap();
    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancel_reports_the_recorded_terminal_outcome_and_cleans_generation_state() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-generation-terminal-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let generation = begin_active_generation(&generation_test_sidecar(), 1_000, true).unwrap();
    let generation_id = generation.generation_id.clone();
    let acknowledger = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if generation_cancel_requested(&generation_id).unwrap() {
                write_generation_terminal_record(&generation_id, "completed", "event-done")
                    .unwrap();
                release_generation_admission(&generation_id).unwrap();
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("generation cancellation marker가 생성되지 않았습니다.");
    });

    let report = cancel_generation_report().unwrap();
    acknowledger.join().unwrap();

    assert!(report.contains("status: acknowledged"));
    assert!(report.contains("terminal outcome: completed"));
    assert!(report.contains("terminal lifecycle event: event-done"));
    assert!(!backend_state::generation_record_path().exists());
    assert!(!backend_state::generation_lock_path().exists());
    assert!(!backend_state::generation_cancel_path().exists());
    assert!(!backend_state::generation_terminal_path(&generation.generation_id).exists());
    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn parallel_generation_cancel_reaches_secondary_and_keeps_state_until_last_release() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-generation-group-cancel-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let sidecar = generation_test_sidecar();
    let primary = begin_active_generation(&sidecar, 1_000, false).unwrap();
    let secondary = begin_active_generation(&sidecar, 1_000, false).unwrap();
    assert_eq!(
        backend_state::read_generation_record()
            .unwrap()
            .unwrap()
            .generation_id,
        primary.generation_id
    );
    write_generation_terminal_record(&primary.generation_id, "completed", "event-primary").unwrap();
    release_generation_admission(&primary.generation_id).unwrap();
    assert!(backend_state::generation_record_path().exists());

    let primary_id = primary.generation_id.clone();
    let secondary_id = secondary.generation_id.clone();
    let secondary_acknowledger = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if generation_cancel_requested(&secondary_id).unwrap() {
                write_generation_terminal_record(&secondary_id, "cancelled", "event-secondary")
                    .unwrap();
                let both_terminal_while_active =
                    backend_state::generation_terminal_path(&primary_id).exists()
                        && backend_state::generation_terminal_path(&secondary_id).exists()
                        && backend_state::generation_record_path().exists();
                release_generation_admission(&secondary_id).unwrap();
                return both_terminal_while_active;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("secondary generation이 primary cancel marker를 관찰하지 못했습니다.");
    });

    let report = cancel_generation_report().unwrap();
    assert!(secondary_acknowledger.join().unwrap());

    assert!(report.contains("status: acknowledged"));
    assert!(!backend_state::generation_record_path().exists());
    assert!(!backend_state::generation_lock_path().exists());
    assert!(!backend_state::generation_cancel_path().exists());
    backend_state::remove_generation_terminal_record(&secondary.generation_id).unwrap();
    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn generation_stop_waits_for_terminal_acknowledgement_before_returning() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-generation-stop-order-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let generation = BackendGenerationRecord {
        generation_id: "generation-stop-order".to_string(),
        client_pid: std::process::id(),
        sidecar_pid: std::process::id(),
        started_at_ms: now_ms(),
        timeout_ms: 1_000,
        streaming_display: true,
    };
    backend_state::acquire_generation_lock(&generation).unwrap();
    write_backend_generation_record(&generation).unwrap();
    let generation_id = generation.generation_id.clone();
    let acknowledger = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if generation_cancel_requested(&generation_id).unwrap() {
                write_generation_terminal_record(
                    &generation_id,
                    "cancelled",
                    "event-stop-cancelled",
                )
                .unwrap();
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("backend stop cancellation marker가 생성되지 않았습니다.");
    });

    let outcome = cancel_active_generation_before_stop(&generation_test_sidecar()).unwrap();
    acknowledger.join().unwrap();

    assert_eq!(outcome, "cancelled");
    assert!(!backend_state::generation_record_path().exists());
    assert!(!backend_state::generation_lock_path().exists());
    assert!(!backend_state::generation_cancel_path().exists());
    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_generation_start_publishes_exactly_one_owner() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-generation-race-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("project")).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    let sidecar = Arc::new(generation_test_sidecar());
    let barrier = Arc::new(Barrier::new(3));
    let contenders = (0..2)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let sidecar = Arc::clone(&sidecar);
            thread::spawn(move || {
                barrier.wait();
                begin_active_generation(&sidecar, 1_000, false)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let results = contenders
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    let admitted = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .collect::<Vec<_>>();

    assert_eq!(admitted.len(), 2);
    let active = backend_state::read_generation_record().unwrap().unwrap();
    let lock = backend_state::read_generation_lock_record()
        .unwrap()
        .unwrap();
    assert!(admitted
        .iter()
        .any(|generation| generation.generation_id == active.generation_id));
    assert_eq!(lock.generation_id, active.generation_id);
    release_generation_admission(&admitted[0].generation_id).unwrap();
    assert_eq!(
        backend_state::read_generation_record()
            .unwrap()
            .unwrap()
            .generation_id,
        active.generation_id
    );
    release_generation_admission(&admitted[1].generation_id).unwrap();
    assert!(!backend_state::generation_record_path().exists());
    assert!(!backend_state::generation_lock_path().exists());
    let next = begin_active_generation(&sidecar, 1_000, false).unwrap();
    release_generation_admission(&next.generation_id).unwrap();
    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    fs::remove_dir_all(root).unwrap();
}
