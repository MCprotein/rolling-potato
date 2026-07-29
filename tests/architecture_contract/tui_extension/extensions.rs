include!("extensions/plugin_storage.rs");

#[test]
fn v03711_extension_owners_hold_manifests_lifecycle_and_admission_policy() {
    let hook = "src/runtime_core/extensions/hook.rs";
    let hook_codec = "src/runtime_core/extensions/hook/codec.rs";
    let hook_policy = "src/runtime_core/extensions/hook/policy.rs";
    let hook_registry = "src/runtime_core/extensions/hook/registry.rs";
    let hook_report = "src/runtime_core/extensions/hook/report.rs";
    let hook_tests = "src/runtime_core/extensions/hook/tests.rs";
    let hook_types = "src/runtime_core/extensions/hook/types.rs";
    let skill = "src/runtime_core/extensions/skill.rs";
    let skill_builtin = "src/runtime_core/extensions/skill/builtin.rs";
    let skill_lifecycle = "src/runtime_core/extensions/skill/lifecycle.rs";
    let skill_manifest = "src/runtime_core/extensions/skill/manifest.rs";
    let skill_policy = "src/runtime_core/extensions/skill/policy.rs";
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
        hook_codec,
        hook_policy,
        hook_registry,
        hook_report,
        hook_tests,
        hook_types,
        skill,
        skill_builtin,
        skill_lifecycle,
        skill_manifest,
        skill_policy,
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

    let hook_facade = fs::read_to_string(hook).unwrap();
    for owner in ["codec", "policy", "registry", "report", "types"] {
        assert!(
            hook_facade
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "hook facade does not register responsibility owner: {owner}"
        );
    }
    let hook_codec_source = fs::read_to_string(hook_codec).unwrap();
    let hook_policy_source = fs::read_to_string(hook_policy).unwrap();
    let hook_registry_source = fs::read_to_string(hook_registry).unwrap();
    let hook_report_source = fs::read_to_string(hook_report).unwrap();
    let hook_types_source = fs::read_to_string(hook_types).unwrap();
    for (owner, rules) in [
        (hook_codec_source.as_str(), &["fn parse_hook_status"][..]),
        (
            hook_policy_source.as_str(),
            &["fn dispatch", "fn resolve_conflict"][..],
        ),
        (
            hook_registry_source.as_str(),
            &["struct HookPoint", "const HOOK_POINTS"][..],
        ),
        (
            hook_report_source.as_str(),
            &["fn list_report", "fn validate_result_report"][..],
        ),
        (
            hook_types_source.as_str(),
            &["enum HookStatus", "struct HookRule"][..],
        ),
    ] {
        for rule in rules {
            assert!(
                owner.contains(rule),
                "hook responsibility owner is missing: {rule}"
            );
            assert!(
                !hook_facade.contains(rule),
                "hook facade still owns behavior: {rule}"
            );
        }
    }
    for (source, maximum_lines, owner) in [
        (hook_facade.as_str(), 30, hook),
        (hook_codec_source.as_str(), 50, hook_codec),
        (hook_policy_source.as_str(), 175, hook_policy),
        (hook_registry_source.as_str(), 125, hook_registry),
        (hook_report_source.as_str(), 100, hook_report),
        (hook_types_source.as_str(), 100, hook_types),
    ] {
        for dependency in [
            "crate::adapters",
            "crate::ledger",
            "crate::plugin",
            "crate::skill",
            "crate::state",
            "std::fs",
            "std::process",
        ] {
            assert!(
                !source.contains(dependency),
                "extension owner has concrete reverse dependency: {owner} -> {dependency}"
            );
        }
        assert!(
            source.lines().count() < maximum_lines,
            "hook owner regrew beyond its responsibility boundary: {owner}"
        );
    }
    let skill_facade = fs::read_to_string(skill).unwrap();
    let skill_builtin_source = fs::read_to_string(skill_builtin).unwrap();
    let skill_lifecycle_source = fs::read_to_string(skill_lifecycle).unwrap();
    let skill_manifest_source = fs::read_to_string(skill_manifest).unwrap();
    let skill_policy_source = fs::read_to_string(skill_policy).unwrap();
    for owner in ["builtin", "lifecycle", "manifest", "policy"] {
        assert!(
            skill_facade
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "skill facade does not register {owner}"
        );
    }
    for (owner, rules) in [
        (
            skill_manifest_source.as_str(),
            &["struct SkillManifest", "enum ResolvedSkillManifest"][..],
        ),
        (
            skill_lifecycle_source.as_str(),
            &["struct SkillRuntimeState", "fn validate_transition"][..],
        ),
        (
            skill_builtin_source.as_str(),
            &["const BUILTIN_SKILLS", "fn find_skill"][..],
        ),
        (
            skill_policy_source.as_str(),
            &["fn enforce_resolved_context", "fn enforce_resolved_tool"][..],
        ),
    ] {
        for rule in rules {
            assert!(
                owner.contains(rule),
                "skill responsibility owner is missing: {rule}"
            );
            assert!(
                !skill_facade.contains(rule),
                "skill facade still owns behavior: {rule}"
            );
        }
    }
    for (source, maximum_lines, owner) in [
        (skill_facade.as_str(), 30, skill),
        (skill_builtin_source.as_str(), 175, skill_builtin),
        (skill_lifecycle_source.as_str(), 200, skill_lifecycle),
        (skill_manifest_source.as_str(), 175, skill_manifest),
        (skill_policy_source.as_str(), 75, skill_policy),
    ] {
        for dependency in [
            "crate::adapters",
            "crate::hooks",
            "crate::plugin",
            "crate::state",
            "std::fs",
            "std::process",
        ] {
            assert!(
                !source.contains(dependency),
                "skill domain has concrete reverse dependency: {owner} -> {dependency}"
            );
        }
        assert!(
            source.lines().count() < maximum_lines,
            "skill owner regrew beyond its responsibility boundary: {owner}"
        );
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
    assert_plugin_storage_contract(&plugin_adapter, plugin_registry, &plugin_scanner);
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
