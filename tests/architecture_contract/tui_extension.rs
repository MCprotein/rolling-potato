use super::*;

include!("tui_extension/tui.rs");
include!("tui_extension/extensions.rs");

#[test]
fn tui_extension_contracts_are_split_by_responsibility() {
    for (path, maximum_lines) in [
        ("tests/architecture_contract/tui_extension.rs", 50),
        ("tests/architecture_contract/tui_extension/tui.rs", 275),
        (
            "tests/architecture_contract/tui_extension/extensions.rs",
            475,
        ),
    ] {
        assert!(
            Path::new(path).is_file(),
            "missing TUI/extension contract owner: {path}"
        );
        assert!(
            fs::read_to_string(path).unwrap().lines().count() < maximum_lines,
            "TUI/extension contract owner regrew beyond its boundary: {path}"
        );
    }
}
