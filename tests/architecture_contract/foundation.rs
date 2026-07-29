use super::*;

#[test]
fn architecture_roots_are_compile_connected_and_private() {
    let main = fs::read_to_string("src/main.rs").expect("src/main.rs must be readable");
    assert!(
        !Path::new("src/lib.rs").exists(),
        "binary-only package must not expose an unapproved library API"
    );
    for root in ARCHITECTURE_ROOTS {
        assert!(main.lines().any(|line| line == format!("mod {root};")));
        assert!(!main.lines().any(|line| line == format!("pub mod {root};")));
    }

    let english = fs::read_to_string("docs/code-architecture.md").unwrap();
    let korean = fs::read_to_string("docs/ko/code-architecture.md").unwrap();
    assert!(english.contains("[코드 아키텍처](ko/code-architecture.md)"));
    assert!(english.contains("[architecture-migration-map.json](architecture-migration-map.json)"));
    assert!(korean.contains("[Code architecture](../code-architecture.md)"));
    assert!(
        korean.contains("[architecture-migration-map.json](../architecture-migration-map.json)")
    );
}

#[test]
fn v0372_foundation_owners_replace_legacy_modules() {
    for target in [
        "src/foundation/error.rs",
        "src/foundation/integrity.rs",
        "src/foundation/serialization.rs",
        "src/foundation/serialization/parser.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing foundation owner: {target}"
        );
    }
    for legacy in ["src/checksum.rs", "src/strict_json.rs"] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy foundation owner remains: {legacy}"
        );
    }

    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_module in ["checksum", "strict_json"] {
        assert!(
            !main
                .lines()
                .any(|line| line == format!("mod {legacy_module};")),
            "legacy module remains compile-connected: {legacy_module}"
        );
    }

    let foundation = fs::read_to_string("src/foundation/mod.rs").unwrap();
    for owner in ["error", "integrity", "serialization"] {
        assert!(
            foundation
                .lines()
                .any(|line| line == format!("pub(crate) mod {owner};")),
            "foundation owner is not crate-private: {owner}"
        );
    }

    let serialization = fs::read_to_string("src/foundation/serialization.rs").unwrap();
    let parser = fs::read_to_string("src/foundation/serialization/parser.rs").unwrap();
    assert!(serialization.contains("#[path = \"serialization/parser.rs\"]"));
    assert!(serialization.lines().any(|line| line == "mod parser;"));
    for responsibility in [
        "pub(super) fn parse_value(",
        "struct Parser<'a>",
        "fn value(",
        "fn object(",
        "fn array(",
        "fn string_value(",
        "fn number_value(",
        "fn hex4(",
    ] {
        assert!(
            parser.contains(responsibility),
            "strict JSON parser owner is missing: {responsibility}"
        );
        assert!(
            !serialization.contains(responsibility),
            "serialization facade still owns parser implementation: {responsibility}"
        );
    }
    assert!(serialization.lines().count() < 525);
    assert!(parser.lines().count() < 300);

    let app = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        !app.contains("pub struct AppError"),
        "AppError is still owned by command dispatch"
    );
}

#[test]
fn v0372_filesystem_owners_replace_legacy_modules() {
    for target in [
        "src/adapters/filesystem/atomic_write.rs",
        "src/adapters/filesystem/cache.rs",
        "src/adapters/filesystem/config.rs",
        "src/adapters/filesystem/layout.rs",
        "src/adapters/filesystem/lease.rs",
        "src/adapters/filesystem/lease/identity.rs",
        "src/adapters/filesystem/windows_replace.rs",
        "src/composition/config.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing filesystem owner: {target}"
        );
    }
    for legacy in [
        "src/cache.rs",
        "src/config.rs",
        "src/lease.rs",
        "src/paths.rs",
        "src/windows_file.rs",
    ] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy filesystem owner remains: {legacy}"
        );
    }

    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_module in ["cache", "config", "lease", "paths", "windows_file"] {
        assert!(
            !main
                .lines()
                .any(|line| line == format!("mod {legacy_module};")),
            "legacy module remains compile-connected: {legacy_module}"
        );
    }

    let filesystem = fs::read_to_string("src/adapters/filesystem/mod.rs").unwrap();
    for owner in [
        "atomic_write",
        "cache",
        "config",
        "layout",
        "lease",
        "windows_replace",
    ] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            filesystem.lines().any(|line| line == expected),
            "filesystem owner is not crate-private: {owner}"
        );
    }

    let lease = fs::read_to_string("src/adapters/filesystem/lease.rs").unwrap();
    let lease_identity = fs::read_to_string("src/adapters/filesystem/lease/identity.rs").unwrap();
    assert!(lease.lines().any(|line| line == "mod identity;"));
    for responsibility in [
        "fn remove_stale_owner_claims(",
        "fn open_owner_namespace_guard(",
        "fn validate_open_owner_namespace_identity(",
        "fn owner_claim_directory(",
        "fn reject_non_regular_lock_path(",
        "fn validate_open_lock_identity(",
    ] {
        assert!(
            lease_identity.contains(responsibility),
            "filesystem lease identity owner is missing: {responsibility}"
        );
        assert!(
            !lease.contains(responsibility),
            "filesystem lease orchestration still owns identity I/O: {responsibility}"
        );
    }
    assert!(lease.lines().count() < 425);
    assert!(lease_identity.lines().count() < 300);
}

#[test]
fn v0372_terminal_and_platform_owners_replace_legacy_modules() {
    for target in [
        "src/adapters/terminal/capability.rs",
        "src/adapters/terminal/native.rs",
        "src/adapters/terminal/native/live_input.rs",
        "src/adapters/terminal/native/platform.rs",
        "src/adapters/terminal/native/platform/capability.rs",
        "src/adapters/terminal/native/platform/unix.rs",
        "src/adapters/terminal/native/platform/windows.rs",
        "src/adapters/terminal/native/platform/unsupported.rs",
        "src/surfaces/tui/command_palette.rs",
        "tests/surfaces.rs",
        "tests/surfaces/interactive_tui.rs",
        "tests/surfaces/native_terminal.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing terminal owner: {target}"
        );
    }
    for legacy in [
        "src/terminal.rs",
        "tests/interactive_tui.rs",
        "tests/native_terminal.rs",
    ] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy terminal owner remains: {legacy}"
        );
    }

    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(
        !main.lines().any(|line| line == "mod terminal;"),
        "legacy terminal module remains compile-connected"
    );

    let terminal = fs::read_to_string("src/adapters/terminal/mod.rs").unwrap();
    for owner in ["capability", "native"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            terminal.lines().any(|line| line == expected),
            "terminal owner is not crate-private: {owner}"
        );
    }

    let native = fs::read_to_string("src/adapters/terminal/native.rs").unwrap();
    let platform = fs::read_to_string("src/adapters/terminal/native/platform.rs").unwrap();
    let platform_capability =
        fs::read_to_string("src/adapters/terminal/native/platform/capability.rs").unwrap();
    let unix_platform =
        fs::read_to_string("src/adapters/terminal/native/platform/unix.rs").unwrap();
    let windows_platform =
        fs::read_to_string("src/adapters/terminal/native/platform/windows.rs").unwrap();
    let unsupported_platform =
        fs::read_to_string("src/adapters/terminal/native/platform/unsupported.rs").unwrap();
    let live_input_editor =
        fs::read_to_string("src/adapters/terminal/native/live_input/editor.rs").unwrap();
    assert!(
        native.lines().any(|line| line == "mod platform;"),
        "native terminal adapter does not register its platform owner"
    );
    assert!(
        native.lines().any(|line| line == "mod live_input;"),
        "native terminal adapter does not register its live input owner"
    );
    assert!(platform.lines().any(|line| line == "mod capability;"));
    for owner in ["unix.rs", "windows.rs", "unsupported.rs"] {
        assert!(platform.contains(&format!("#[path = \"platform/{owner}\"]")));
    }
    for owner in [&unix_platform, &windows_platform, &unsupported_platform] {
        for responsibility in ["pub fn dimensions(", "pub fn read_secret("] {
            assert!(owner.contains(responsibility));
            assert!(!platform.contains(responsibility));
        }
    }
    assert!(unix_platform.contains("unsafe extern \"C\""));
    assert!(unix_platform.contains("fn restore_echo_before_signal_exit("));
    assert!(windows_platform.contains("unsafe extern \"system\""));
    assert!(windows_platform.contains("fn restore_echo_before_console_exit("));
    assert!(platform_capability.contains("const LIVE_INPUT: bool"));
    assert!(native.lines().count() < 325);
    for owner in [
        &platform,
        &platform_capability,
        &unix_platform,
        &windows_platform,
        &unsupported_platform,
    ] {
        assert!(owner.lines().count() < 500);
    }
    let live_input = fs::read_to_string("src/adapters/terminal/native/live_input.rs").unwrap();
    for responsibility in [
        "pub(super) fn read(",
        "fn matching_suggestions<'a>(",
        "fn redraw(",
    ] {
        assert!(
            live_input.contains(responsibility),
            "native live input owner is missing: {responsibility}"
        );
        assert!(
            !native.contains(responsibility),
            "native terminal facade owns live input behavior: {responsibility}"
        );
    }
    assert!(
        live_input_editor.contains("fn pop_last_utf8_char("),
        "native live input editor is missing its UTF-8 byte regression helper"
    );
    assert!(
        !live_input.contains("fn pop_last_utf8_char("),
        "native live input facade still owns editor-specific UTF-8 byte behavior"
    );
    assert!(live_input.lines().count() < 300);
}
