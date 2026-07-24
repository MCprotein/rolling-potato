use std::fs;

use super::*;
use crate::adapters::filesystem::{backend_state, model_artifact};
use crate::runtime_core::inference::model::codec::parse_registry_entry;

const MODEL_SHA256: &str = "9372c470eeadd5ecd9c3c74c2b3cb633f8e2f2fad799250a0f70d652b6b825e4";
const PROJECTOR_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn descriptor(
    file_name: &'static str,
    sha256: &'static str,
    size_bytes: u64,
) -> ModelArtifactDescriptor {
    ModelArtifactDescriptor {
        provider: "test",
        url: "https://example.invalid/artifact.gguf",
        terms_url: "https://example.invalid/terms",
        file_name,
        sha256,
        size_bytes,
    }
}

fn legacy_registry(model_path: &Path) -> String {
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"id\": \"fixture\",\n  \"displayName\": \"Fixture\",\n  \"status\": \"installed\",\n  \"evidenceStatus\": \"verified-local-promotion\",\n  \"promotionEvidencePath\": \"/evidence/promotion.json\",\n  \"backendVersion\": \"backend-1\",\n  \"benchmarkRunId\": \"benchmark-1\",\n  \"upstreamModel\": \"owner/model\",\n  \"upstreamUrl\": \"https://example.invalid/model\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"licenseSource\": \"https://example.invalid/license\",\n  \"licenseCheckedAt\": \"2026-07-24\"\n}}\n",
        model_path.display(),
        MODEL_SHA256
    )
}

fn seed_preserved_state(root: &Path, registry: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let registry_path = model_artifact::registry_path("fixture");
    fs::create_dir_all(registry_path.parent().unwrap()).unwrap();
    fs::write(&registry_path, registry).unwrap();

    let default_path = model_artifact::paths().default_file;
    fs::write(&default_path, b"default-selection-sentinel").unwrap();
    let backend_path = backend_state::sidecar_record_path();
    fs::create_dir_all(backend_path.parent().unwrap()).unwrap();
    fs::write(&backend_path, b"backend-state-sentinel").unwrap();
    assert!(root.exists());
    (default_path, backend_path)
}

fn restore_data_home(previous: Option<std::ffi::OsString>, root: &Path) {
    if let Some(previous) = previous {
        std::env::set_var("RPOTATO_DATA_HOME", previous);
    } else {
        std::env::remove_var("RPOTATO_DATA_HOME");
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn model_upgrade_compatibility_image_use_migrates_v1_binding_and_preserves_state() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-projector-binding-success-{}",
        std::process::id()
    ));
    let previous = std::env::var_os("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_DATA_HOME", &root);
    fs::create_dir_all(&root).unwrap();

    let model_path = root.join("fixture-model.gguf");
    let projector_path = root.join("fixture-projector.gguf");
    let part_path = root.join("fixture-projector.gguf.part");
    fs::write(&model_path, b"model").unwrap();
    fs::write(&projector_path, b"abc").unwrap();
    let legacy = legacy_registry(&model_path);
    let (default_path, backend_path) = seed_preserved_state(&root, &legacy);

    let prepared = prepare_bound_vision_projector_artifacts(
        "fixture",
        descriptor("fixture-model.gguf", MODEL_SHA256, 5),
        &model_path,
        descriptor("fixture-projector.gguf", PROJECTOR_SHA256, 3),
        &projector_path,
        &part_path,
    )
    .unwrap();
    let prepared_again = prepare_bound_vision_projector_artifacts(
        "fixture",
        descriptor("fixture-model.gguf", MODEL_SHA256, 5),
        &model_path,
        descriptor("fixture-projector.gguf", PROJECTOR_SHA256, 3),
        &projector_path,
        &part_path,
    )
    .unwrap();

    assert_eq!(prepared, prepared_again);
    assert_eq!(prepared.path, projector_path);
    assert!(!part_path.exists());
    let entry = parse_registry_entry(
        &fs::read_to_string(model_artifact::registry_path("fixture")).unwrap(),
    )
    .unwrap();
    assert_eq!(entry.vision_status, "ready");
    assert_eq!(entry.mmproj_path.as_deref(), prepared.path.to_str());
    assert_eq!(entry.mmproj_sha256.as_deref(), Some(PROJECTOR_SHA256));
    assert_eq!(entry.mmproj_size_bytes, Some(3));
    assert_eq!(entry.evidence_status, "verified-local-promotion");
    assert_eq!(entry.promotion_evidence_path, "/evidence/promotion.json");
    assert_eq!(entry.backend_version, "backend-1");
    assert_eq!(entry.benchmark_run_id, "benchmark-1");
    assert_eq!(
        fs::read(default_path).unwrap(),
        b"default-selection-sentinel"
    );
    assert_eq!(fs::read(backend_path).unwrap(), b"backend-state-sentinel");
    restore_data_home(previous, &root);
}

#[test]
fn model_upgrade_compatibility_preparation_failure_preserves_registry_default_and_backend() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-projector-binding-failure-{}",
        std::process::id()
    ));
    let previous = std::env::var_os("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_DATA_HOME", &root);
    fs::create_dir_all(&root).unwrap();

    let model_path = root.join("fixture-model.gguf");
    let projector_path = root.join("fixture-projector.gguf");
    let part_path = root.join("fixture-projector.gguf.part");
    fs::write(&model_path, b"corrupt").unwrap();
    fs::write(&projector_path, b"abc").unwrap();
    let legacy = legacy_registry(&model_path);
    let (default_path, backend_path) = seed_preserved_state(&root, &legacy);

    let error = prepare_bound_vision_projector_artifacts(
        "fixture",
        descriptor("fixture-model.gguf", MODEL_SHA256, 5),
        &model_path,
        descriptor("fixture-projector.gguf", PROJECTOR_SHA256, 3),
        &projector_path,
        &part_path,
    )
    .unwrap_err();

    assert!(error
        .message
        .contains("text-ready backend는 변경하지 않습니다"));
    assert_eq!(
        fs::read_to_string(model_artifact::registry_path("fixture")).unwrap(),
        legacy
    );
    assert_eq!(
        fs::read(default_path).unwrap(),
        b"default-selection-sentinel"
    );
    assert_eq!(fs::read(backend_path).unwrap(), b"backend-state-sentinel");
    assert_eq!(fs::read(projector_path).unwrap(), b"abc");
    assert!(!part_path.exists());
    restore_data_home(previous, &root);
}
