use super::*;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn termination_fallback_forces_a_process_after_graceful_command_failure() {
    let calls = std::cell::RefCell::new(Vec::new());
    let running = std::cell::Cell::new(true);

    terminate_with_fallback(
        || {
            calls.borrow_mut().push("graceful");
            Err(AppError::runtime("graceful unsupported"))
        },
        || {
            calls.borrow_mut().push("force");
            running.set(false);
            Ok(())
        },
        || Ok(running.get()),
        || Ok(!running.get()),
        42,
    )
    .unwrap();

    assert_eq!(*calls.borrow(), ["graceful", "force"]);
    assert!(!running.get());
}

#[test]
fn termination_fallback_accepts_force_race_when_process_is_already_gone() {
    let running = std::cell::Cell::new(true);

    terminate_with_fallback(
        || Err(AppError::runtime("graceful unsupported")),
        || {
            running.set(false);
            Err(AppError::runtime("process already exited"))
        },
        || Ok(running.get()),
        || Ok(!running.get()),
        43,
    )
    .unwrap();

    assert!(!running.get());
}

#[test]
fn termination_fallback_fails_closed_when_liveness_check_fails() {
    let force_called = std::cell::Cell::new(false);

    let error = terminate_with_fallback(
        || Err(AppError::runtime("graceful unsupported")),
        || {
            force_called.set(true);
            Ok(())
        },
        || Err(AppError::runtime("liveness unavailable")),
        || Ok(false),
        44,
    )
    .unwrap_err();

    assert!(error.message.contains("liveness unavailable"));
    assert!(!force_called.get());
}
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
        host: DEFAULT_HOST.to_string(),
        port: DEFAULT_PORT,
        ctx_size: Some(4096),
        stdout_log: PathBuf::from("stdout.log"),
        stderr_log: PathBuf::from("stderr.log"),
        started_at_ms: now_ms(),
    }
}

#[test]
fn default_discovery_uses_managed_path() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    env::remove_var(ENV_BACKEND_PATH);
    env::remove_var(ENV_BACKEND_PORT);
    let data_root = env::temp_dir().join(format!("rpotato-backend-test-{}", std::process::id()));
    env::set_var("RPOTATO_DATA_HOME", &data_root);

    let discovery = llama_backend::discover();

    env::remove_var("RPOTATO_DATA_HOME");
    assert_eq!(discovery.adapter_id, "llama.cpp");
    assert_eq!(discovery.selected_source, "managed");
    assert!(discovery
        .selected_path
        .ends_with(LlamaCppAdapter.binary_name()));
    assert_eq!(discovery.port, DEFAULT_PORT);
}

#[test]
fn backend_path_and_port_can_come_from_env() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let override_path = env::temp_dir().join("custom-llama-server");
    env::set_var(ENV_BACKEND_PATH, &override_path);
    env::set_var(ENV_BACKEND_PORT, "19090");

    let discovery = llama_backend::discover();

    env::remove_var(ENV_BACKEND_PATH);
    env::remove_var(ENV_BACKEND_PORT);
    assert_eq!(discovery.selected_path, override_path);
    assert_eq!(discovery.selected_source, "env override");
    assert_eq!(discovery.port, 19090);
    assert_eq!(discovery.port_source, "env override");
}

#[test]
fn invalid_backend_port_falls_back_to_default() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    env::set_var(ENV_BACKEND_PORT, "0");

    let discovery = llama_backend::discover();

    env::remove_var(ENV_BACKEND_PORT);
    assert_eq!(discovery.port, DEFAULT_PORT);
    assert_eq!(discovery.port_source, "invalid env, default");
}

#[test]
fn release_manifest_has_source_backed_supported_artifacts() {
    let expected = [
        (
            "macos",
            "aarch64",
            "llama-b9982-bin-macos-arm64.tar.gz",
            "9606e3a609bc9483730f50f17ce78c3d764df8eaec63fcbb47d2f8b235667c9c",
            10_746_432,
            BackendArchiveKind::TarGz,
            "llama-server",
        ),
        (
            "macos",
            "x86_64",
            "llama-b9982-bin-macos-x64.tar.gz",
            "da109cc18574392ab88936de826ca00f8d196b9ef5a1c19da72fbfb06bea7cd0",
            11_022_427,
            BackendArchiveKind::TarGz,
            "llama-server",
        ),
        (
            "linux",
            "aarch64",
            "llama-b9982-bin-ubuntu-arm64.tar.gz",
            "9468c0282c15e286216a63122e7471f7d14888d3858bdab61b72d14a2531cf60",
            12_782_598,
            BackendArchiveKind::TarGz,
            "llama-server",
        ),
        (
            "linux",
            "x86_64",
            "llama-b9982-bin-ubuntu-x64.tar.gz",
            "0c1f0445f6f86a0f049de3586b7eabdde7108d827d0a9b2c5c0dc2185506ffee",
            15_850_588,
            BackendArchiveKind::TarGz,
            "llama-server",
        ),
        (
            "windows",
            "aarch64",
            "llama-b9982-bin-win-cpu-arm64.zip",
            "11ad20d8df121d5760900b4e2fa9943a065856075ef44df52ed7a8dc58b08b2f",
            12_151_247,
            BackendArchiveKind::Zip,
            "llama-server.exe",
        ),
        (
            "windows",
            "x86_64",
            "llama-b9982-bin-win-cpu-x64.zip",
            "69337038e8e56feb3c04d99588fa19f9241b294bae6f6c2e665a301605726e2a",
            18_247_652,
            BackendArchiveKind::Zip,
            "llama-server.exe",
        ),
    ];

    for (
        os,
        arch,
        archive_name,
        archive_sha256,
        archive_size_bytes,
        archive_kind,
        binary_relative_path,
    ) in expected
    {
        let artifact = release_artifact_for(&LLAMA_CPP_RELEASE, os, arch)
            .unwrap_or_else(|| panic!("{os}/{arch} backend artifact should be recorded"));
        assert_eq!(artifact.archive_name, archive_name);
        assert_eq!(
            artifact.archive_url,
            format!(
                "https://github.com/ggml-org/llama.cpp/releases/download/{}/{}",
                LLAMA_CPP_RELEASE.release_tag, artifact.archive_name
            )
        );
        assert_eq!(artifact.archive_sha256, archive_sha256);
        assert_eq!(artifact.archive_size_bytes, archive_size_bytes);
        assert_eq!(artifact.archive_kind, archive_kind);
        assert_eq!(artifact.binary_relative_path, binary_relative_path);
        assert_eq!(
            backend_install_blockers(&LLAMA_CPP_RELEASE, Some(artifact)),
            Vec::<String>::new()
        );
    }
}

#[test]
fn install_plan_uses_current_platform_manifest_when_supported() {
    let report = install_plan_report();

    if selected_backend_release_artifact(&LLAMA_CPP_RELEASE).is_some() {
        assert!(report.contains("status: ready"));
        assert!(report.contains("archive sha256: "));
        assert!(report.contains(&format!("release tag: {}", LLAMA_CPP_RELEASE.release_tag)));
    } else {
        assert!(report.contains("status: blocked"));
        assert!(report.contains("지원 platform artifact 미확정"));
    }
}

#[test]
fn release_artifact_selection_rejects_unknown_platform() {
    assert!(release_artifact_for(&LLAMA_CPP_RELEASE, "freebsd", "riscv64").is_none());
}

#[test]
fn install_from_tar_archive_places_managed_payload() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-backend-install-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    let archive_path = root.join("backend.tar.gz");
    write_test_tar_gz(
        &archive_path,
        &[
            ("release/bin/llama-server", b"fake backend".as_slice()),
            ("release/bin/libllama.dylib", b"fake dylib".as_slice()),
        ],
    )
    .unwrap();

    let artifact = BackendReleaseArtifact {
        os: "test",
        arch: "test",
        archive_name: "backend.tar.gz",
        archive_url: "https://example.invalid/backend.tar.gz",
        archive_sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        archive_size_bytes: archive_path.metadata().unwrap().len(),
        archive_kind: BackendArchiveKind::TarGz,
        binary_relative_path: "llama-server",
    };
    let managed_binary = root.join("managed").join("llama-server");
    let staging_dir = root.join("staging");

    let result = install_backend_from_archive(
        &artifact,
        &archive_path,
        &managed_binary,
        &staging_dir,
        ArchiveDownloadStatus::CacheHit,
    )
    .unwrap();

    assert!(managed_binary.is_file());
    assert!(llama_backend::is_executable(&managed_binary));
    assert_eq!(fs::read(&managed_binary).unwrap(), b"fake backend");
    assert_eq!(
        fs::read(managed_binary.parent().unwrap().join("libllama.dylib")).unwrap(),
        b"fake dylib"
    );
    assert_eq!(result.managed_binary, managed_binary);
    assert!(!staging_dir.exists());
    env::remove_var("RPOTATO_DATA_HOME");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn doctor_skips_version_for_env_override_binary() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    env::set_var(ENV_BACKEND_PATH, "/tmp/user-owned-llama-server");

    let report = doctor_report();

    env::remove_var(ENV_BACKEND_PATH);
    assert!(report.contains("version detection: skipped"));
    assert!(report.contains("env override backend binary"));
}

#[cfg(unix)]
#[test]
fn doctor_runs_version_for_recorded_managed_binary() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = env::temp_dir().join(format!(
        "rpotato-backend-version-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    env::set_var("RPOTATO_DATA_HOME", &root);

    let artifact = selected_backend_release_artifact(&LLAMA_CPP_RELEASE).unwrap();
    let managed_binary = LlamaCppAdapter.managed_binary_path();
    fs::create_dir_all(managed_binary.parent().unwrap()).unwrap();
    let expected_version = format!("llama.cpp fake version {}", LLAMA_CPP_RELEASE.release_tag);
    fs::write(
        &managed_binary,
        format!("#!/bin/sh\necho '{expected_version}'\n"),
    )
    .unwrap();
    llama_install::set_executable_bit(&managed_binary).unwrap();
    let binary_sha256 = checksum::sha256_file(&managed_binary).unwrap();
    llama_install::write_install_record(artifact, &binary_sha256).unwrap();

    let report = doctor_report();

    env::remove_var("RPOTATO_DATA_HOME");
    fs::remove_dir_all(root).unwrap();
    assert!(report.contains("version detection: ok"));
    assert!(report.contains(&expected_version));
}

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

    env::remove_var("RPOTATO_DATA_HOME");
    env::remove_var("RPOTATO_PROJECT_ROOT");
    fs::remove_dir_all(root).unwrap();
    assert!(report.contains("status: stopped"));
}

#[test]
fn sidecar_record_round_trip_preserves_ctx_size() {
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
        mmproj: "not-required-text-only".to_string(),
        host: DEFAULT_HOST.to_string(),
        port: DEFAULT_PORT,
        ctx_size: Some(4096),
        stdout_log: root.join("stdout.log"),
        stderr_log: root.join("stderr.log"),
        started_at_ms: now_ms(),
    };
    backend_state::write_sidecar_record(&record).unwrap();
    let expected = format!(
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

#[cfg(unix)]
#[test]
fn unix_pid_arg_rejects_wrapping_values() {
    assert_eq!(backend_process::unix_pid_arg(0), None);
    assert_eq!(backend_process::unix_pid_arg(u32::MAX), None);
    assert_eq!(
        backend_process::unix_pid_arg(i32::MAX as u32),
        Some((i32::MAX as u32).to_string())
    );
}

#[test]
fn health_check_report_is_diagnostic_not_process_start() {
    let report = health_check_report();
    assert!(report.contains("backend health check"));
    assert!(report.contains("health URL"));
    assert!(report.contains("timeout ms"));
}

#[test]
fn model_id_comes_from_model_file_stem() {
    let model_id = model_id_from_path(Path::new("/tmp/Qwen3.5-4B-Q4_K_M.gguf"));

    assert_eq!(model_id, "Qwen3.5-4B-Q4_K_M");
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
