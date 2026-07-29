use super::*;

include!("adapter_boundaries/application.rs");
include!("adapter_boundaries/github_release.rs");
include!("adapter_boundaries/platform_codec.rs");
include!("adapter_boundaries/state.rs");

#[test]
fn adapter_boundary_contracts_are_split_by_responsibility() {
    for (path, maximum_lines) in [
        ("tests/architecture_contract/adapter_boundaries.rs", 50),
        (
            "tests/architecture_contract/adapter_boundaries/application.rs",
            225,
        ),
        (
            "tests/architecture_contract/adapter_boundaries/github_release.rs",
            125,
        ),
        (
            "tests/architecture_contract/adapter_boundaries/platform_codec.rs",
            225,
        ),
        (
            "tests/architecture_contract/adapter_boundaries/state.rs",
            525,
        ),
    ] {
        assert!(
            Path::new(path).is_file(),
            "missing adapter boundary owner: {path}"
        );
        assert!(
            fs::read_to_string(path).unwrap().lines().count() < maximum_lines,
            "adapter boundary owner regrew beyond its boundary: {path}"
        );
    }
}
