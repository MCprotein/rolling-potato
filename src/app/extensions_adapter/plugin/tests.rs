use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn codex_import_dry_run_reads_manifest() {
    let root = test_plugin_root("codex");
    let manifest_dir = root.join(".codex-plugin");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("plugin.json"),
        r#"{"name":"hello-plugin","version":"1.0.0","description":"hello"}"#,
    )
    .unwrap();

    let report = import_report(PluginSource::Codex, root.to_str().unwrap(), true).unwrap();

    assert!(report.contains("hello-plugin"));
    assert!(report.contains("dry-run"));
    assert!(report.contains("source manifest sha256"));
    assert!(report.contains("source snapshot sha256"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn codex_import_reports_capabilities_and_blocked_permissions() {
    let root = test_plugin_root("codex-capabilities");
    let manifest_dir = root.join(".codex-plugin");
    let skill_dir = root.join("skills").join("review");
    let mcp_dir = root.join("mcp");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::create_dir_all(&skill_dir).unwrap();
    fs::create_dir_all(&mcp_dir).unwrap();
    fs::write(
        manifest_dir.join("plugin.json"),
        r#"{"name":"cap-plugin","version":"1.0.0","description":"cap"}"#,
    )
    .unwrap();
    fs::write(skill_dir.join("SKILL.md"), "# Review\n").unwrap();
    fs::write(mcp_dir.join("server.json"), "{}\n").unwrap();
    fs::write(root.join("background-task.sh"), "#!/usr/bin/env sh\n").unwrap();

    let report = import_report(PluginSource::Codex, root.to_str().unwrap(), true).unwrap();

    assert!(report.contains("skill:skills/review/SKILL.md"));
    assert!(report.contains("mcp-server"));
    assert!(report.contains("shell-command"));
    assert!(report.contains("background-process"));
    assert!(report.contains("blocked permissions"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn codex_import_persists_manifest_and_registry() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let data_root = test_plugin_root("data-root");
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", test_plugin_root("project-root"));

    let root = test_plugin_root("codex-persist");
    let manifest_dir = root.join(".codex-plugin");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("plugin.json"),
        r#"{"name":"hello-plugin","version":"1.0.0","description":"hello"}"#,
    )
    .unwrap();

    let report = import_report(PluginSource::Codex, root.to_str().unwrap(), false).unwrap();
    assert!(report.contains("imported.codex.hello-plugin"));
    assert!(list_report().contains("imported.codex.hello-plugin"));
    assert!(inspect_report("imported.codex.hello-plugin")
        .unwrap()
        .contains("hello-plugin"));
    let normalized =
        fs::read_to_string(normalized_manifest_path("imported.codex.hello-plugin")).unwrap();
    assert!(normalized.contains("\"schemaVersion\": 2"));
    assert!(normalized.contains("\"sourceManifestSha256\""));
    assert!(normalized.contains("\"sourceSnapshotSha256\""));
    assert!(normalized.contains("\"blockedPermissions\""));

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(data_root).unwrap();
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
}

#[test]
fn validate_blocks_imported_source_drift() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let data_root = test_plugin_root("data-root-drift");
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var(
        "RPOTATO_PROJECT_ROOT",
        test_plugin_root("project-root-drift"),
    );

    let root = test_plugin_root("codex-drift");
    let manifest_dir = root.join(".codex-plugin");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(
        manifest_dir.join("plugin.json"),
        r#"{"name":"drift-plugin","version":"1.0.0","description":"drift"}"#,
    )
    .unwrap();

    import_report(PluginSource::Codex, root.to_str().unwrap(), false).unwrap();
    fs::write(
        plugin_dir("imported.codex.drift-plugin")
            .join("source")
            .join(".codex-plugin")
            .join("plugin.json"),
        r#"{"name":"drift-plugin","version":"1.0.1","description":"drift"}"#,
    )
    .unwrap();

    let err = validate_report("imported.codex.drift-plugin").unwrap_err();

    assert_eq!(err.code, 3);
    assert!(err.message.contains("validation blocked"));
    assert!(inspect_report("imported.codex.drift-plugin")
        .unwrap()
        .contains("status: blocked"));

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(data_root).unwrap();
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
}

#[test]
fn enabled_instruction_only_codex_skill_resolves() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_codex_skill_plugin(
        "safe-skill",
        "safe-plugin",
        "hello",
        "---\nname: hello\ndescription: 답변 형식을 안내한다.\n---\n요청을 한국어로 요약하세요.\n",
        None,
    );

    import_report(PluginSource::Codex, root.to_str().unwrap(), false).unwrap();
    validate_report("imported.codex.safe-plugin").unwrap();
    set_enabled_report("imported.codex.safe-plugin", true).unwrap();

    let skill = resolve_imported_codex_skill("imported.codex.safe-plugin.hello")
        .unwrap()
        .unwrap();
    assert_eq!(skill.plugin_id, "imported.codex.safe-plugin");
    assert_eq!(skill.source_path, "skills/hello/SKILL.md");
    assert_eq!(skill.instructions, "요청을 한국어로 요약하세요.");
    assert_eq!(skill.source_sha256.len(), 64);
    assert!(skill::list_report().contains("imported.codex.safe-plugin.hello"));

    cleanup_codex_skill_test(root, data_root);
}

#[test]
fn disabled_codex_skill_cannot_resolve() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_codex_skill_plugin(
        "disabled-skill",
        "disabled-plugin",
        "hello",
        "---\nname: hello\ndescription: 안내\n---\n요청을 요약하세요.\n",
        None,
    );

    import_report(PluginSource::Codex, root.to_str().unwrap(), false).unwrap();
    validate_report("imported.codex.disabled-plugin").unwrap();

    let error = resolve_imported_codex_skill("imported.codex.disabled-plugin.hello").unwrap_err();
    assert_eq!(error.code, 3);
    assert!(error.message.contains("status: validated"));

    cleanup_codex_skill_test(root, data_root);
}

#[test]
fn codex_skill_without_frontmatter_is_blocked() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_codex_skill_plugin(
        "missing-frontmatter",
        "frontmatter-plugin",
        "hello",
        "# Hello\n요청을 요약하세요.\n",
        None,
    );

    import_report(PluginSource::Codex, root.to_str().unwrap(), false).unwrap();
    set_enabled_report("imported.codex.frontmatter-plugin", true).unwrap();

    let error =
        resolve_imported_codex_skill("imported.codex.frontmatter-plugin.hello").unwrap_err();
    assert_eq!(error.code, 3);
    assert!(error.message.contains("YAML frontmatter가 없습니다"));

    cleanup_codex_skill_test(root, data_root);
}

#[test]
fn codex_skill_with_script_is_blocked_by_default() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_codex_skill_plugin(
        "script-skill",
        "script-plugin",
        "hello",
        "---\nname: hello\ndescription: 안내\n---\n요청을 요약하세요.\n",
        Some(("run.py", "print('unsafe')\n")),
    );

    let report = import_report(PluginSource::Codex, root.to_str().unwrap(), false).unwrap();
    assert!(report.contains("skill-script"));
    set_enabled_report("imported.codex.script-plugin", true).unwrap();

    let error = resolve_imported_codex_skill("imported.codex.script-plugin.hello").unwrap_err();
    assert_eq!(error.code, 3);
    assert!(error.message.contains("별도 승인이 필요한 실행 capability"));

    cleanup_codex_skill_test(root, data_root);
}

#[test]
fn tampered_normalized_capability_summary_cannot_admit_scripted_skill() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_codex_skill_plugin(
        "tampered-capabilities",
        "tampered-plugin",
        "hello",
        "---\nname: hello\ndescription: 안내\n---\n요청을 요약하세요.\n",
        Some(("run.py", "print('unsafe')\n")),
    );

    import_report(PluginSource::Codex, root.to_str().unwrap(), false).unwrap();
    set_enabled_report("imported.codex.tampered-plugin", true).unwrap();
    let manifest_path = normalized_manifest_path("imported.codex.tampered-plugin");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let tampered = manifest.replace(
        "skill-script|skills/hello/scripts/run.py|blocked-by-default|skill-script",
        "skill-script|skills/hello/scripts/run.py|mapped|none",
    );
    assert_ne!(tampered, manifest);
    fs::write(&manifest_path, tampered).unwrap();

    let error = resolve_imported_codex_skill("imported.codex.tampered-plugin.hello").unwrap_err();
    assert_eq!(error.code, 3);
    assert!(error.message.contains("normalized capability metadata"));
    assert!(inspect_report("imported.codex.tampered-plugin")
        .unwrap()
        .contains("status: blocked"));

    cleanup_codex_skill_test(root, data_root);
}

#[test]
fn skill_discovery_uses_slugged_plugin_id() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_codex_skill_plugin(
        "slugged-discovery",
        "My Plugin_v1.0",
        "hello",
        "---\nname: hello\ndescription: 안내\n---\n요청을 요약하세요.\n",
        None,
    );

    import_report(PluginSource::Codex, root.to_str().unwrap(), false).unwrap();
    set_enabled_report("imported.codex.my-plugin-v1-0", true).unwrap();

    let report = skill::list_report();
    assert!(report.contains("imported.codex.my-plugin-v1-0.hello"));
    assert!(!report.contains("imported.codex.My Plugin_v1.0.hello"));

    cleanup_codex_skill_test(root, data_root);
}

#[test]
fn claude_code_import_reports_adapter_surfaces() {
    let root = test_plugin_root("claude-capabilities");
    let manifest_dir = root.join(".claude-plugin");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::create_dir_all(root.join("commands")).unwrap();
    fs::create_dir_all(root.join("agents")).unwrap();
    fs::create_dir_all(root.join("hooks")).unwrap();
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("monitors")).unwrap();
    fs::create_dir_all(root.join("lsp")).unwrap();
    fs::write(
        manifest_dir.join("plugin.json"),
        r#"{"name":"claude-plugin","version":"1.0.0","description":"claude"}"#,
    )
    .unwrap();
    fs::write(root.join("commands").join("review.md"), "# Review\n").unwrap();
    fs::write(root.join("agents").join("critic.md"), "# Critic\n").unwrap();
    fs::write(root.join("hooks").join("stop.sh"), "#!/usr/bin/env sh\n").unwrap();
    fs::write(root.join("bin").join("tool.sh"), "#!/usr/bin/env sh\n").unwrap();
    fs::write(root.join("monitors").join("watch.json"), "{}\n").unwrap();
    fs::write(root.join("lsp").join("server.json"), "{}\n").unwrap();
    fs::write(root.join("settings.json"), "{}\n").unwrap();

    let report = import_report(PluginSource::ClaudeCode, root.to_str().unwrap(), true).unwrap();

    assert!(report.contains("command:commands"));
    assert!(report.contains("subagent:agents"));
    assert!(report.contains("hook"));
    assert!(report.contains("bin-executable"));
    assert!(report.contains("background-process"));
    assert!(report.contains("lsp-server"));
    assert!(report.contains("runtime-settings"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn remote_plugin_import_is_blocked() {
    let err =
        import_report(PluginSource::Codex, "https://example.com/plugin.git", true).unwrap_err();

    assert_eq!(err.code, 3);
    assert!(err.message.contains("remote URL"));
}

#[test]
fn path_traversal_plugin_import_is_blocked() {
    let err = import_report(PluginSource::Codex, "../plugin", true).unwrap_err();
    assert_eq!(err.code, 3);
}

#[test]
fn missing_manifest_is_usage_error() {
    let root = test_plugin_root("missing-manifest");
    fs::create_dir_all(&root).unwrap();

    let err = import_report(PluginSource::ClaudeCode, root.to_str().unwrap(), true).unwrap_err();

    assert_eq!(err.code, 2);
    assert!(err.message.contains("manifest"));

    fs::remove_dir_all(root).unwrap();
}

fn test_plugin_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rpotato-plugin-test-{}-{}-{unique}",
        std::process::id(),
        label
    ))
}

fn prepare_codex_skill_plugin(
    label: &str,
    plugin_name: &str,
    skill_name: &str,
    skill_text: &str,
    script: Option<(&str, &str)>,
) -> (PathBuf, PathBuf) {
    let data_root = test_plugin_root(&format!("{label}-data"));
    let project_root = test_plugin_root(&format!("{label}-project"));
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    let root = test_plugin_root(label);
    let manifest_dir = root.join(".codex-plugin");
    let skill_dir = root.join("skills").join(skill_name);
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        manifest_dir.join("plugin.json"),
        format!(r#"{{"name":"{plugin_name}","version":"1.0.0","description":"test"}}"#),
    )
    .unwrap();
    fs::write(skill_dir.join("SKILL.md"), skill_text).unwrap();
    if let Some((name, text)) = script {
        let scripts = skill_dir.join("scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(scripts.join(name), text).unwrap();
    }
    (root, data_root)
}

fn cleanup_codex_skill_test(root: PathBuf, data_root: PathBuf) {
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(data_root).unwrap();
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
}
