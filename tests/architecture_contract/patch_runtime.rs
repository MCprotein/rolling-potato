use super::*;

include!("patch_runtime/patch_domain_lifecycle.rs");
include!("patch_runtime/application_adapter_contracts.rs");
include!("patch_runtime/intent_adapter.rs");
include!("patch_runtime/runtime_reporting.rs");

#[test]
fn patch_runtime_contract_children_are_bounded() {
    for (owner, limit) in [
        (
            "tests/architecture_contract/patch_runtime/patch_domain_lifecycle.rs",
            250,
        ),
        (
            "tests/architecture_contract/patch_runtime/application_adapter_contracts.rs",
            500,
        ),
        (
            "tests/architecture_contract/patch_runtime/intent_adapter.rs",
            125,
        ),
        (
            "tests/architecture_contract/patch_runtime/runtime_reporting.rs",
            300,
        ),
    ] {
        let source = fs::read_to_string(owner)
            .unwrap_or_else(|error| panic!("cannot read patch runtime owner {owner}: {error}"));
        assert!(
            source.lines().count() < limit,
            "patch runtime contract owner regrew beyond its boundary: {owner}"
        );
    }
}
