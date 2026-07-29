#[test]
fn transcript_compatibility_codec_has_bounded_responsibility_owners() {
    let root = "src/runtime_core/workflow/storage_compat/transcript.rs";
    let facade = fs::read_to_string(root).unwrap();
    let owners = [
        ("schema", 75, "const TRANSCRIPT_V2_KEYS"),
        ("types", 75, "pub struct TranscriptRecord"),
        ("encode", 175, "fn canonical_install_bytes"),
        ("decode", 250, "fn parse_record"),
        ("validation", 175, "fn validate_tool_binding_shape"),
    ];

    assert!(
        facade.lines().count() < 50,
        "transcript compatibility facade regrew beyond stable exports"
    );
    for (owner, line_budget, responsibility) in owners {
        let relative = format!("transcript/{owner}.rs");
        assert!(
            facade.contains(&relative),
            "transcript facade does not register {owner}"
        );
        let source = fs::read_to_string(format!(
            "src/runtime_core/workflow/storage_compat/{relative}"
        ))
        .unwrap();
        assert!(
            source.contains(responsibility),
            "transcript owner {owner} is missing {responsibility}"
        );
        assert!(
            source.lines().count() < line_budget,
            "transcript owner {owner} exceeded its {line_budget}-line budget"
        );
    }

    for moved_responsibility in [
        "pub struct TranscriptRecord",
        "fn canonical_install_bytes",
        "fn parse_record",
        "fn validate_tool_binding_shape",
    ] {
        assert!(
            !facade.contains(moved_responsibility),
            "transcript facade still owns {moved_responsibility}"
        );
    }
}

#[test]
fn transcript_compatibility_codec_remains_a_pure_storage_boundary() {
    for owner in [
        "src/runtime_core/workflow/storage_compat/transcript.rs",
        "src/runtime_core/workflow/storage_compat/transcript/schema.rs",
        "src/runtime_core/workflow/storage_compat/transcript/types.rs",
        "src/runtime_core/workflow/storage_compat/transcript/encode.rs",
        "src/runtime_core/workflow/storage_compat/transcript/decode.rs",
        "src/runtime_core/workflow/storage_compat/transcript/validation.rs",
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for forbidden in [
            "use std::fs",
            "OpenOptions",
            "std::io::Write",
            ".exists()",
            "fs::read",
            "fs::create_dir",
        ] {
            assert!(
                !source.contains(forbidden),
                "transcript codec owns filesystem behavior: {owner} -> {forbidden}"
            );
        }
    }
}
