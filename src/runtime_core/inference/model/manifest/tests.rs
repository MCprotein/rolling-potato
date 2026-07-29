use super::*;

#[test]
fn catalog_counts_and_lookup_are_stable() {
    assert_eq!(ManifestCounts::from_candidates().total, 3);
    assert_eq!(find_candidate("qwen3.5-4b").unwrap().format, "gguf");
    assert!(find_candidate("unknown-model").is_err());
}

#[test]
fn source_backed_fetch_is_separate_from_install_readiness() {
    let candidate = find_candidate("qwen3.5-4b").unwrap();
    assert!(!validate_install_ready(candidate).ready);
    assert!(source_backed_artifact(candidate).is_ok());

    let incomplete = find_candidate("qwen3.5-9b").unwrap();
    assert!(source_backed_artifact(incomplete).is_err());
}

#[test]
fn quantization_lookup_tracks_source_backed_manifest_hashes() {
    assert_eq!(
        quantization_for_artifact_hash(
            "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4"
        ),
        Some("Q4_K_M")
    );
    assert_eq!(quantization_for_artifact_hash(&"f".repeat(64)), None);
}
