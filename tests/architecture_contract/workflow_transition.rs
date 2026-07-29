use super::*;

include!("workflow_transition/application.rs");
include!("workflow_transition/transition_adapter.rs");

#[test]
fn workflow_transition_contracts_are_split_by_responsibility() {
    for (path, maximum_lines) in [
        ("tests/architecture_contract/workflow_transition.rs", 50),
        (
            "tests/architecture_contract/workflow_transition/application.rs",
            350,
        ),
        (
            "tests/architecture_contract/workflow_transition/transition_adapter.rs",
            350,
        ),
    ] {
        assert!(
            Path::new(path).is_file(),
            "missing workflow transition contract owner: {path}"
        );
        assert!(
            fs::read_to_string(path).unwrap().lines().count() < maximum_lines,
            "workflow transition contract owner regrew beyond its boundary: {path}"
        );
    }
}
