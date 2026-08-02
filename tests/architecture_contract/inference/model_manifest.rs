fn assert_model_manifest_owners() {
    let facade = fs::read_to_string("src/runtime_core/inference/model/manifest.rs").unwrap();
    let profiles =
        fs::read_to_string("src/runtime_core/inference/model/manifest/profiles.rs").unwrap();
    let types = fs::read_to_string("src/runtime_core/inference/model/manifest/types.rs").unwrap();
    let validation =
        fs::read_to_string("src/runtime_core/inference/model/manifest/validation.rs").unwrap();
    let tests = fs::read_to_string("src/runtime_core/inference/model/manifest/tests.rs").unwrap();

    for owner in ["profiles", "types", "validation"] {
        assert!(
            facade.contains(&format!("#[path = \"manifest/{owner}.rs\"]")),
            "model manifest facade does not register {owner}"
        );
    }
    assert!(
        facade.contains("#[path = \"manifest/tests.rs\"]"),
        "model manifest facade does not register its regression-test owner"
    );

    assert!(types.contains("pub(crate) struct ModelManifestEntry"));
    assert!(profiles.contains("QWEN_4B_GENERATION"));
    assert!(profiles.contains("GEMMA_4B_GENERATION"));
    assert!(types.contains("pub(crate) struct PromotionEvidence"));
    assert!(validation.contains("pub(crate) fn find_candidate("));
    assert!(validation.contains("pub(crate) fn validate_install_ready("));
    assert!(tests.contains("fn source_backed_fetch_is_separate_from_install_readiness("));
    for moved_responsibility in [
        "pub(crate) struct ModelManifestEntry",
        "pub(crate) fn find_candidate(",
        "fn source_backed_fetch_is_separate_from_install_readiness(",
    ] {
        assert!(
            !facade.contains(moved_responsibility),
            "model manifest facade still owns moved responsibility: {moved_responsibility}"
        );
    }

    assert!(
        facade.lines().count() < 250,
        "model manifest catalog facade regrew beyond its ownership boundary"
    );
    assert!(
        types.lines().count() < 225,
        "model manifest data contracts regrew beyond their ownership boundary"
    );
    assert!(
        profiles.lines().count() < 125,
        "model behavior profiles need responsibility-based splitting"
    );
    assert!(
        validation.lines().count() < 225,
        "model manifest validation policy regrew beyond its ownership boundary"
    );
    assert!(
        tests.lines().count() < 75,
        "model manifest regression tests regrew beyond their ownership boundary"
    );
}
