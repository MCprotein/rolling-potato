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
