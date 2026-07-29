#[test]
fn ontology_contracts_are_split_by_runtime_responsibility() {
    let ontology = fs::read_to_string("src/runtime_core/knowledge/ontology.rs").unwrap();
    assert!(ontology.lines().count() < 175);
    for (owner, line_budget, rules) in [
        (
            "projection",
            100,
            &["fn parse_projection", "fn seeded_record_changed"][..],
        ),
        (
            "selection",
            225,
            &[
                "fn diagnostics_from_projection",
                "fn runtime_context_selection",
            ][..],
        ),
        (
            "import",
            100,
            &["fn validate_import_text", "confirmed Layer B"][..],
        ),
        (
            "codec",
            200,
            &["fn to_json_line", "fn parse", "fn stable_id"][..],
        ),
        ("tests", 100, &["ontology_record_bytes_are_stable"][..]),
    ] {
        let relative = format!("ontology/{owner}.rs");
        assert!(
            ontology.contains(&relative),
            "ontology facade does not register {owner}"
        );
        let source = fs::read_to_string(format!("src/runtime_core/knowledge/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "ontology owner {owner} exceeded its {line_budget}-line budget"
        );
        for rule in rules {
            assert!(
                source.contains(rule),
                "ontology owner {owner} is missing responsibility: {rule}"
            );
        }
        for forbidden in ["crate::adapters", "crate::ledger", "crate::state"] {
            assert!(
                !source.contains(forbidden),
                "ontology owner has concrete reverse dependency: {owner} -> {forbidden}"
            );
        }
    }
}
