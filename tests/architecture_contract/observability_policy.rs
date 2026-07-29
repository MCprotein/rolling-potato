use super::*;

include!("observability_policy/observability.rs");
include!("observability_policy/ontology_adapter.rs");
include!("observability_policy/knowledge_policy.rs");
include!("observability_policy/ontology.rs");

#[test]
fn observability_policy_contracts_are_split_by_responsibility() {
    for (path, maximum_lines) in [
        ("tests/architecture_contract/observability_policy.rs", 60),
        (
            "tests/architecture_contract/observability_policy/observability.rs",
            25,
        ),
        (
            "tests/architecture_contract/observability_policy/observability/boundaries.rs",
            100,
        ),
        (
            "tests/architecture_contract/observability_policy/observability/runtime.rs",
            200,
        ),
        (
            "tests/architecture_contract/observability_policy/observability/sqlite.rs",
            375,
        ),
        (
            "tests/architecture_contract/observability_policy/observability/sqlite/analytics.rs",
            125,
        ),
        (
            "tests/architecture_contract/observability_policy/knowledge_policy.rs",
            275,
        ),
        (
            "tests/architecture_contract/observability_policy/ontology.rs",
            100,
        ),
        (
            "tests/architecture_contract/observability_policy/ontology_adapter.rs",
            150,
        ),
    ] {
        assert!(
            Path::new(path).is_file(),
            "missing observability/policy contract owner: {path}"
        );
        assert!(
            fs::read_to_string(path).unwrap().lines().count() < maximum_lines,
            "observability/policy contract owner regrew beyond its boundary: {path}"
        );
    }
}
