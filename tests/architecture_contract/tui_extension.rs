use super::*;

#[test]
fn v03713_storage_compatibility_is_a_pure_codec_boundary() {
    for owner in [
        "src/runtime_core/workflow/storage_compat/ledger.rs",
        "src/runtime_core/workflow/storage_compat/transcript.rs",
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
                "storage compatibility codec owns filesystem behavior: {owner} -> {forbidden}"
            );
        }
    }
}

#[test]
fn v0471_tui_controller_support_responsibilities_are_split() {
    let controller = fs::read_to_string("src/surfaces/tui/controller.rs").unwrap();
    for (module, owner, marker) in [
        (
            "attachments",
            "src/surfaces/tui/controller/attachments.rs",
            "fn looks_like_attachment_path",
        ),
        (
            "model_selection",
            "src/surfaces/tui/controller/model_selection.rs",
            "fn choose_model",
        ),
        (
            "session_selection",
            "src/surfaces/tui/controller/session_selection.rs",
            "fn resume_session",
        ),
        (
            "terminal_flow",
            "src/surfaces/tui/controller/terminal_flow.rs",
            "fn terminal_fault_error",
        ),
    ] {
        assert!(
            controller
                .lines()
                .any(|line| line == format!("mod {module};")),
            "TUI controller does not register support owner: {module}"
        );
        let source = fs::read_to_string(owner).unwrap();
        assert!(
            source.contains(marker),
            "TUI controller support owner is missing responsibility: {owner} -> {marker}"
        );
    }
    assert!(
        controller.lines().count() < 500,
        "TUI controller regrew beyond command-loop ownership"
    );
}

#[test]
fn tui_conversation_journeys_have_bounded_feature_owners() {
    let root = fs::read_to_string("src/app/tui_adapter/conversation_tests.rs").unwrap();
    let owners = [
        ("attachment_layout", 175),
        ("progress_model", 175),
        ("rendering", 175),
        ("session", 200),
        ("web", 175),
    ];

    assert!(root.lines().count() < 300);
    for (owner, line_budget) in owners {
        let relative = format!("conversation_tests/{owner}.rs");
        assert!(
            root.contains(&relative),
            "conversation journey root does not register {owner}"
        );
        let source = fs::read_to_string(format!("src/app/tui_adapter/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "conversation journey {owner} exceeded its {line_budget}-line budget"
        );
    }
}

#[test]
fn tui_attachment_capture_and_composition_have_bounded_owners() {
    let root = fs::read_to_string("src/app/tui_adapter/attachment.rs").unwrap();
    let owners = [
        ("capture", 200, "fn capture("),
        ("compose", 250, "fn compose_request("),
        ("format", 125, "fn attachment_kind("),
        ("path", 75, "fn normalized_source_path("),
        ("tests", 350, "captures_text_into_app_data"),
    ];

    assert!(root.lines().count() < 75);
    for (owner, line_budget, marker) in owners {
        let relative = format!("attachment/{owner}.rs");
        assert!(
            root.contains(&relative) || root.contains(&format!("mod {owner};")),
            "attachment facade does not register {owner}"
        );
        let source = fs::read_to_string(format!("src/app/tui_adapter/{relative}")).unwrap();
        assert!(
            source.contains(marker),
            "attachment owner {owner} is missing {marker}"
        );
        assert!(
            source.lines().count() < line_budget,
            "attachment owner {owner} exceeded its {line_budget}-line budget"
        );
    }
}

#[test]
fn terminal_live_input_has_a_semantic_keymap_owner() {
    let input = fs::read_to_string("src/adapters/terminal/native/live_input.rs").unwrap();
    let keymap = fs::read_to_string("src/adapters/terminal/native/live_input/keymap.rs").unwrap();

    assert!(input.lines().any(|line| line == "mod keymap;"));
    assert!(keymap.contains("pub(super) enum Action"));
    assert!(keymap.contains("pub(super) fn decode_escape"));
    assert!(!input.contains("enum Action"));
    assert!(input.lines().count() < 250);
    assert!(keymap.lines().count() < 100);
}

#[test]
fn tui_render_responsibilities_have_bounded_owners() {
    let render = fs::read_to_string("src/surfaces/tui/render.rs").unwrap();
    for (module, owner, marker, line_budget) in [
        (
            "chrome",
            "src/surfaces/tui/render/chrome.rs",
            "fn render_composer",
            200,
        ),
        (
            "conversation",
            "src/surfaces/tui/render/conversation.rs",
            "fn render_frame",
            225,
        ),
        (
            "notice",
            "src/surfaces/tui/render/notice.rs",
            "fn render_lines",
            75,
        ),
        (
            "report_layout",
            "src/surfaces/tui/render/report_layout.rs",
            "fn push_wrapped",
            175,
        ),
        (
            "text",
            "src/surfaces/tui/render/text.rs",
            "fn sanitize_terminal_text",
            175,
        ),
    ] {
        assert!(
            render.lines().any(|line| line == format!("mod {module};")),
            "TUI render does not register responsibility owner: {module}"
        );
        let source = fs::read_to_string(owner).unwrap();
        assert!(
            source.contains(marker),
            "TUI render responsibility owner is missing: {owner} -> {marker}"
        );
        assert!(
            source.lines().count() < line_budget,
            "TUI render responsibility owner exceeded its {line_budget}-line budget: {owner}"
        );
    }
    assert!(
        render.lines().count() < 150,
        "TUI render facade regrew beyond frame routing ownership"
    );
}

#[test]
fn v0471_patch_intent_classification_and_model_action_are_split() {
    let intent = fs::read_to_string("src/runtime_core/patch/intent.rs").unwrap();
    for (module, owner, marker) in [
        (
            "classification",
            "src/runtime_core/patch/intent/classification.rs",
            "fn classify",
        ),
        (
            "model_action",
            "src/runtime_core/patch/intent/model_action.rs",
            "fn parse_model_action",
        ),
    ] {
        assert!(
            intent.lines().any(|line| line == format!("mod {module};")),
            "patch intent does not register responsibility owner: {module}"
        );
        let source = fs::read_to_string(owner).unwrap();
        assert!(
            source.contains(marker),
            "patch intent responsibility owner is missing: {owner} -> {marker}"
        );
    }
    assert!(
        intent.lines().count() < 300,
        "patch intent facade regrew beyond contracts and action planning"
    );
}

#[test]
fn v03711_extension_owners_hold_manifests_lifecycle_and_admission_policy() {
    let hook = "src/runtime_core/extensions/hook.rs";
    let skill = "src/runtime_core/extensions/skill.rs";
    let plugin = "src/runtime_core/extensions/plugin.rs";
    let plugin_capabilities = "src/runtime_core/extensions/plugin/capabilities.rs";
    let plugin_json = "src/runtime_core/extensions/plugin/json.rs";
    let plugin_parsing = "src/runtime_core/extensions/plugin/parsing.rs";
    let plugin_security = "src/runtime_core/extensions/plugin/security.rs";
    let hooks_adapter = "src/app/extensions_adapter/hooks.rs";
    let plugin_adapter = "src/app/extensions_adapter/plugin.rs";
    let plugin_claude = "src/app/extensions_adapter/plugin/claude.rs";
    let plugin_execution = "src/app/extensions_adapter/plugin/execution.rs";
    let plugin_registry = "src/app/extensions_adapter/plugin/registry.rs";
    let plugin_scanner = "src/app/extensions_adapter/plugin/scanner.rs";
    let plugin_source_import = "src/app/extensions_adapter/plugin/source_import.rs";
    let plugin_tests = "src/app/extensions_adapter/plugin/tests.rs";
    let skill_adapter = "src/app/extensions_adapter/skill.rs";
    for target in [
        hook,
        skill,
        plugin,
        plugin_capabilities,
        plugin_json,
        plugin_parsing,
        plugin_security,
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.11 extension owner: {target}"
        );
    }

    let extensions_mod = fs::read_to_string("src/runtime_core/extensions/mod.rs").unwrap();
    for child in ["hook", "skill", "plugin"] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            extensions_mod.lines().any(|line| line == expected),
            "extension child is not crate-private: {child}"
        );
    }

    for (owner, rules, forbidden) in [
        (
            hook,
            [
                "enum HookStatus",
                "struct HookRule",
                "const HOOK_POINTS",
                "fn dispatch",
                "fn resolve_conflict",
            ]
            .as_slice(),
            [
                "crate::adapters",
                "crate::ledger",
                "crate::plugin",
                "crate::skill",
                "crate::state",
                "std::fs",
                "std::process",
            ]
            .as_slice(),
        ),
        (
            skill,
            [
                "struct SkillManifest",
                "enum ResolvedSkillManifest",
                "struct SkillRuntimeState",
                "fn validate_transition",
                "fn enforce_resolved_tool",
            ]
            .as_slice(),
            [
                "crate::adapters",
                "crate::hooks",
                "crate::plugin",
                "crate::state",
                "std::fs",
                "std::process",
            ]
            .as_slice(),
        ),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for rule in rules {
            assert!(
                source.contains(rule),
                "v0.37.11 owner is missing extension rule: {owner} -> {rule}"
            );
        }
        for dependency in forbidden {
            assert!(
                !source.contains(dependency),
                "extension owner has concrete reverse dependency: {owner} -> {dependency}"
            );
        }
    }
    let plugin_facade = fs::read_to_string(plugin).unwrap();
    let plugin_capabilities_source = fs::read_to_string(plugin_capabilities).unwrap();
    let plugin_json_source = fs::read_to_string(plugin_json).unwrap();
    let plugin_parsing_source = fs::read_to_string(plugin_parsing).unwrap();
    let plugin_security_source = fs::read_to_string(plugin_security).unwrap();
    for module in [
        "mod capabilities;",
        "mod json;",
        "mod parsing;",
        "mod security;",
    ] {
        assert!(
            plugin_facade.lines().any(|line| line == module),
            "plugin facade does not register owner: {module}"
        );
    }
    for (owner, responsibility) in [
        (
            plugin_capabilities_source.as_str(),
            "struct PluginCapability",
        ),
        (
            plugin_capabilities_source.as_str(),
            "fn apply_manifest_risk_markers",
        ),
        (
            plugin_capabilities_source.as_str(),
            "fn blocked_permissions",
        ),
        (plugin_json_source.as_str(), "fn required_field"),
        (plugin_parsing_source.as_str(), "struct ParsedCodexSkill"),
        (plugin_parsing_source.as_str(), "fn parse_codex_skill"),
        (
            plugin_security_source.as_str(),
            "fn reject_remote_or_marketplace",
        ),
    ] {
        assert!(
            owner.contains(responsibility),
            "plugin responsibility owner is missing: {responsibility}"
        );
        assert!(
            !plugin_facade.contains(responsibility),
            "plugin facade still owns behavior: {responsibility}"
        );
    }
    for source in [
        plugin_facade.as_str(),
        plugin_capabilities_source.as_str(),
        plugin_json_source.as_str(),
        plugin_parsing_source.as_str(),
        plugin_security_source.as_str(),
    ] {
        for dependency in [
            "crate::adapters",
            "crate::cli",
            "crate::ledger",
            "crate::state",
            "std::fs",
            "std::process",
        ] {
            assert!(
                !source.contains(dependency),
                "plugin domain has concrete reverse dependency: {dependency}"
            );
        }
    }
    for (source, maximum_lines, owner) in [
        (plugin_facade.as_str(), 50, plugin),
        (
            plugin_capabilities_source.as_str(),
            275,
            plugin_capabilities,
        ),
        (plugin_json_source.as_str(), 125, plugin_json),
        (plugin_parsing_source.as_str(), 275, plugin_parsing),
        (plugin_security_source.as_str(), 125, plugin_security),
    ] {
        assert!(
            source.lines().count() < maximum_lines,
            "plugin owner regrew beyond its responsibility boundary: {owner}"
        );
    }

    for target in [
        hooks_adapter,
        plugin_adapter,
        plugin_claude,
        plugin_execution,
        plugin_registry,
        plugin_scanner,
        plugin_source_import,
        plugin_tests,
        skill_adapter,
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.13 extension adapter: {target}"
        );
    }
    let adapter_mod = fs::read_to_string("src/app/extensions_adapter.rs").unwrap();
    for child in ["hooks", "plugin", "skill"] {
        let expected = format!("pub(crate) mod {child};");
        assert!(
            adapter_mod.lines().any(|line| line == expected),
            "extension adapter child is not crate-private: {child}"
        );
    }

    for (adapter, moved_definition) in [
        (hooks_adapter, "enum HookStatus"),
        (hooks_adapter, "fn resolve_conflict"),
        (skill_adapter, "struct SkillManifest"),
        (skill_adapter, "fn validate_transition"),
        (plugin_adapter, "struct PluginCapability"),
        (plugin_adapter, "fn parse_codex_skill"),
        (plugin_adapter, "fn apply_manifest_risk_markers"),
    ] {
        let source = fs::read_to_string(adapter).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(moved_definition),
            "extension adapter retains moved rule: {adapter} -> {moved_definition}"
        );
    }

    for legacy in ["src/hooks.rs", "src/plugin.rs", "src/skill.rs"] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy extension root was restored: {legacy}"
        );
    }
    let main = fs::read_to_string("src/main.rs").unwrap();
    for legacy_mod in ["mod hooks;", "mod plugin;", "mod skill;"] {
        assert!(
            !main.lines().any(|line| line == legacy_mod),
            "legacy extension root remains registered: {legacy_mod}"
        );
    }

    let hooks_adapter = fs::read_to_string(hooks_adapter).unwrap();
    let skill_adapter = fs::read_to_string(skill_adapter).unwrap();
    let plugin_adapter = fs::read_to_string(plugin_adapter).unwrap();
    let plugin_execution = fs::read_to_string(plugin_execution).unwrap();
    let plugin_scanner = fs::read_to_string(plugin_scanner).unwrap();
    let plugin_source_import = fs::read_to_string(plugin_source_import).unwrap();
    let plugin_tests = fs::read_to_string(plugin_tests).unwrap();
    assert!(
        plugin_adapter.lines().any(|line| line == "mod execution;"),
        "plugin adapter does not register its execution validation owner"
    );
    for responsibility in [
        "pub fn resolve_imported_skill(",
        "fn resolve_imported_skill_inner(",
        "pub fn revalidate_completed_imported_skill(",
        "fn verify_execution_metadata(",
    ] {
        assert!(
            plugin_execution.contains(responsibility),
            "plugin execution owner is missing: {responsibility}"
        );
        assert!(
            !plugin_adapter.contains(responsibility),
            "plugin adapter still owns execution validation: {responsibility}"
        );
    }
    assert!(
        plugin_adapter.lines().any(|line| line == "mod claude;"),
        "plugin adapter does not register its Claude Code mapping owner"
    );
    let plugin_claude = fs::read_to_string(plugin_claude).unwrap();
    for responsibility in [
        "pub(super) fn classify_file(",
        "pub(super) fn record_directory_semantics(",
        "fn classify_instruction(",
    ] {
        assert!(
            plugin_claude.contains(responsibility),
            "Claude Code mapping owner is missing: {responsibility}"
        );
        assert!(
            !plugin_scanner.contains(responsibility),
            "plugin scanner still owns Claude Code mapping: {responsibility}"
        );
    }
    assert!(
        plugin_adapter
            .lines()
            .any(|line| line == "mod source_import;"),
        "plugin adapter does not register its source import owner"
    );
    for responsibility in [
        "pub(super) struct SourcePlugin",
        "pub(super) fn inspect_source_plugin(",
        "pub(super) fn normalize_plugin(",
    ] {
        assert!(
            plugin_source_import.contains(responsibility),
            "plugin source import owner is missing: {responsibility}"
        );
        assert!(
            !plugin_adapter.contains(responsibility),
            "plugin adapter still owns source import normalization: {responsibility}"
        );
    }
    assert!(
        plugin_adapter.lines().any(|line| line == "mod scanner;"),
        "plugin adapter does not register its scanner owner"
    );
    assert!(
        plugin_adapter.lines().any(|line| line == "mod registry;"),
        "plugin adapter does not register its registry owner"
    );
    let plugin_registry = fs::read_to_string(plugin_registry).unwrap();
    for responsibility in [
        "pub(super) struct PluginSnapshot",
        "pub(super) fn persist_plugin(",
        "pub(super) fn verify_imported_snapshot(",
        "pub(super) fn read_plugins(",
        "pub(super) fn read_plugin(",
        "pub(super) fn write_plugin_manifest(",
        "pub(super) fn write_validation_report(",
    ] {
        assert!(
            plugin_registry.contains(responsibility),
            "plugin registry owner is missing: {responsibility}"
        );
        assert!(
            !plugin_adapter.contains(responsibility),
            "plugin adapter still owns registry behavior: {responsibility}"
        );
    }
    for responsibility in [
        "pub(super) struct DirectoryScan",
        "pub(super) fn scan_directory(",
        "pub(super) fn copy_dir_recursive(",
        "fn classify_runtime_file(",
        "pub(super) fn sha256_directory_snapshot(",
        "fn collect_snapshot_entries(",
    ] {
        assert!(
            plugin_scanner.contains(responsibility),
            "plugin scanner owner is missing: {responsibility}"
        );
        assert!(
            !plugin_adapter.contains(responsibility),
            "plugin adapter still owns scanner behavior: {responsibility}"
        );
    }
    assert!(
        plugin_adapter.contains("#[path = \"plugin/tests.rs\"]"),
        "plugin adapter does not register its regression-test owner"
    );
    for regression in [
        "fn codex_import_persists_manifest_and_registry(",
        "fn validate_blocks_imported_source_drift(",
        "fn tampered_normalized_capability_summary_cannot_admit_scripted_skill(",
        "fn path_traversal_plugin_import_is_blocked(",
    ] {
        assert!(
            plugin_tests.contains(regression),
            "plugin regression owner is missing: {regression}"
        );
        assert!(
            !plugin_adapter.contains(regression),
            "plugin adapter still owns regression test: {regression}"
        );
    }
    assert!(
        hooks_adapter.lines().count() <= 300,
        "hooks adapter regrew beyond the v0.37.13 boundary"
    );
    assert!(
        skill_adapter.lines().count() <= 250,
        "skill adapter regrew beyond the v0.37.13 boundary"
    );
    assert!(
        plugin_adapter.lines().count() < 375,
        "plugin adapter regrew beyond the v0.37.13 boundary"
    );
    assert!(
        plugin_claude.lines().count() < 200,
        "Claude Code mapping module regrew beyond its ownership boundary"
    );
    assert!(
        plugin_execution.lines().count() < 325,
        "plugin execution module regrew beyond its ownership boundary"
    );
    assert!(
        plugin_source_import.lines().count() < 175,
        "plugin source import module regrew beyond its ownership boundary"
    );
    assert!(
        plugin_registry.lines().count() < 350,
        "plugin registry module regrew beyond its ownership boundary"
    );
    assert!(
        plugin_scanner.lines().count() < 450,
        "plugin scanner module regrew beyond its ownership boundary"
    );
    assert!(
        plugin_tests.lines().count() < 450,
        "plugin regression module regrew beyond its ownership boundary"
    );
}
