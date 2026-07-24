use super::*;

#[test]
fn model_upgrade_compatibility_legacy_registry_keeps_text_runtime_and_manifest_context() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-legacy-model-runtime-test-{}",
        std::process::id()
    ));
    let previous_data = std::env::var_os("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("RPOTATO_DATA_HOME", &root);
    let candidate = find_candidate("gemma-4-e4b").unwrap();
    let artifact =
        crate::runtime_core::inference::model::manifest::source_backed_artifact(candidate).unwrap();
    let paths = crate::adapters::filesystem::model_artifact::paths();
    std::fs::create_dir_all(&paths.registry_dir).unwrap();
    let legacy_registry = format!(
        "{{\n  \"schemaVersion\": 1,\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"status\": \"installed\",\n  \"evidenceStatus\": \"source-backed-manifest\",\n  \"promotionEvidencePath\": \"\",\n  \"backendVersion\": \"\",\n  \"benchmarkRunId\": \"\",\n  \"upstreamModel\": \"{}\",\n  \"upstreamUrl\": \"{}\",\n  \"artifactPath\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"licenseSource\": \"{}\",\n  \"licenseCheckedAt\": \"{}\"\n}}\n",
        candidate.id,
        candidate.display_name,
        candidate.upstream_model,
        candidate.upstream_url,
        crate::adapters::filesystem::model_artifact::model_artifact_path(artifact).display(),
        artifact.sha256,
        candidate.license.source,
        candidate.license.checked_at
    );
    let registry_path = crate::adapters::filesystem::model_artifact::registry_path(candidate.id);
    std::fs::write(&registry_path, &legacy_registry).unwrap();
    std::fs::write(
        &paths.default_file,
        format!(
            "{{\n  \"schemaVersion\": 1,\n  \"modelId\": \"{}\",\n  \"artifactSha256\": \"{}\",\n  \"selectedAtMs\": 1\n}}\n",
            candidate.id, artifact.sha256
        ),
    )
    .unwrap();

    let configured = configured_runtime_spec().unwrap();

    assert_eq!(configured.model_id, candidate.id);
    assert_eq!(configured.context_tokens, 131_072);
    assert!(configured.vision_projector_path.is_none());
    assert_eq!(
        std::fs::read_to_string(&registry_path).unwrap(),
        legacy_registry,
        "text-only reconciliation must not rewrite or claim a projector"
    );
    if let Some(previous) = previous_data {
        std::env::set_var("RPOTATO_DATA_HOME", previous);
    } else {
        std::env::remove_var("RPOTATO_DATA_HOME");
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn setup_options_expose_each_models_manifest_context_limit() {
    let options = setup_options();

    assert_eq!(
        options
            .iter()
            .find(|option| option.id == "qwen3.5-4b")
            .and_then(|option| option.context_length),
        Some(262_144)
    );
    assert_eq!(
        options
            .iter()
            .find(|option| option.id == "gemma-4-e4b")
            .and_then(|option| option.context_length),
        Some(131_072)
    );
}
