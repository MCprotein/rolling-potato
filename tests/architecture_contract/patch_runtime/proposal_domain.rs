#[test]
fn patch_proposal_domain_has_bounded_codec_and_preview_owners() {
    let proposal_facade = fs::read_to_string("src/runtime_core/patch/proposal.rs").unwrap();
    assert!(
        proposal_facade.lines().count() < 35,
        "proposal facade regrew beyond stable domain exports"
    );
    for owner in ["encoding", "preview", "record", "types"] {
        assert!(
            proposal_facade
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "proposal facade does not register {owner}"
        );
    }
    assert!(
        proposal_facade.contains("#[path = \"proposal/tests.rs\"]"),
        "proposal facade no longer registers its regression-test owner"
    );
    for responsibility in [
        "struct PatchPreview",
        "struct ProposalRecord",
        "fn build_preview",
        "fn render_unified_diff",
        "fn render_record",
        "fn parse_record",
        "fn validate_proposal_id",
        "fn encode_hex_text",
        "fn sha256_text",
    ] {
        assert!(
            !proposal_facade.contains(responsibility),
            "proposal facade still owns {responsibility}"
        );
    }
    for (owner, line_budget) in [
        ("encoding.rs", 100),
        ("preview.rs", 175),
        ("record.rs", 250),
        ("tests.rs", 100),
        ("types.rs", 125),
    ] {
        let source =
            fs::read_to_string(format!("src/runtime_core/patch/proposal/{owner}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "proposal owner {owner} exceeded its {line_budget}-line budget"
        );
    }
}
