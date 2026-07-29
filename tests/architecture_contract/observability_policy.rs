use super::*;

include!("observability_policy/observability.rs");
include!("observability_policy/knowledge_policy.rs");

#[test]
fn observability_policy_contracts_are_split_by_responsibility() {
    for (path, maximum_lines) in [
        ("tests/architecture_contract/observability_policy.rs", 50),
        (
            "tests/architecture_contract/observability_policy/observability.rs",
            475,
        ),
        (
            "tests/architecture_contract/observability_policy/knowledge_policy.rs",
            275,
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
