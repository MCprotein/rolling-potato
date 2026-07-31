#[test]
fn backend_vision_readiness_distinguishes_support_from_runtime_readiness() {
    let mut record = generation_test_sidecar();
    record.model_sha256 =
        "e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d".to_string();

    assert_eq!(vision_readiness(&record), "on-demand (text-ready)");

    record.mmproj_path = Some(PathBuf::from("/models/mmproj.gguf"));
    assert_eq!(
        vision_readiness(&record),
        "on-demand (text-ready)",
        "an unverified path must never make vision ready"
    );

    record.mmproj_path = None;
    record.model_sha256 = "f".repeat(64);
    assert_eq!(vision_readiness(&record), "unavailable (text-ready)");
}

fn verified_projector_fixture() -> (BackendSidecarRecord, PathBuf, String, u64) {
    let projector_path = PathBuf::from("/models/mmproj.gguf");
    let projector_sha256 = "c".repeat(64);
    let projector_size_bytes = 512;
    let mut record = generation_test_sidecar();
    record.mmproj_path = Some(projector_path.clone());
    record.mmproj_sha256 = Some(projector_sha256.clone());
    record.mmproj_size_bytes = Some(projector_size_bytes);

    (
        record,
        projector_path,
        projector_sha256,
        projector_size_bytes,
    )
}

#[test]
fn backend_runtime_projector_accepts_exact_verified_cached_binding() {
    let (record, projector_path, projector_sha256, projector_size_bytes) =
        verified_projector_fixture();

    assert!(runtime_binding_matches(
        &record,
        &projector_path,
        &projector_sha256,
        projector_size_bytes
    ));
    assert_eq!(supported_vision_readiness(true), "ready");
}

#[test]
fn backend_runtime_projector_rejects_missing_cached_path_without_blocking_text() {
    let (mut record, projector_path, projector_sha256, projector_size_bytes) =
        verified_projector_fixture();

    record.mmproj_path = None;
    assert!(!runtime_binding_matches(
        &record,
        &projector_path,
        &projector_sha256,
        projector_size_bytes
    ));
    assert_eq!(
        supported_vision_readiness(false),
        "on-demand (text-ready)"
    );
}

#[test]
fn backend_runtime_projector_rejects_stale_or_wrong_binding() {
    let (mut record, projector_path, projector_sha256, projector_size_bytes) =
        verified_projector_fixture();

    record.mmproj_sha256 = Some("d".repeat(64));
    assert!(!runtime_binding_matches(
        &record,
        &projector_path,
        &projector_sha256,
        projector_size_bytes
    ));

    record.mmproj_sha256 = Some(projector_sha256.clone());
    record.mmproj_size_bytes = Some(projector_size_bytes + 1);
    assert!(!runtime_binding_matches(
        &record,
        &projector_path,
        &projector_sha256,
        projector_size_bytes
    ));

    record.mmproj_size_bytes = Some(projector_size_bytes);
    record.mmproj_path = Some(PathBuf::from("/models/stale-mmproj.gguf"));
    assert!(!runtime_binding_matches(
        &record,
        &projector_path,
        &projector_sha256,
        projector_size_bytes
    ));
}
