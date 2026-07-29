fn assert_model_codec_owners() {
    let facade_path = "src/runtime_core/inference/model/codec.rs";
    for target in [
        facade_path,
        "src/runtime_core/inference/model/codec/promotion.rs",
        "src/runtime_core/inference/model/codec/registry.rs",
        "src/runtime_core/inference/model/codec/render.rs",
        "src/runtime_core/inference/model/codec/selection.rs",
        "src/runtime_core/inference/model/codec/tests.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing model codec owner: {target}"
        );
    }

    let facade = fs::read_to_string(facade_path).unwrap();
    for (owner, limit, responsibility) in [
        ("promotion", 150, "fn parse_promotion_evidence("),
        ("registry", 150, "fn parse_registry_entry("),
        ("render", 200, "fn render_registry_entry("),
        ("selection", 50, "fn parse_default_selection("),
        (
            "tests",
            200,
            "fn registry_v1_remains_text_ready_without_claiming_vision(",
        ),
    ] {
        assert!(
            facade
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "model codec facade does not register {owner}"
        );
        assert!(
            !facade.contains(responsibility),
            "model codec facade still owns {responsibility}"
        );
        let source =
            fs::read_to_string(format!("src/runtime_core/inference/model/codec/{owner}.rs"))
                .unwrap();
        assert!(
            source.contains(responsibility),
            "model codec owner is missing {responsibility}"
        );
        assert!(
            source.lines().count() < limit,
            "model codec owner {owner} exceeded its line budget"
        );
    }
    assert!(facade.lines().count() < 40);
}
