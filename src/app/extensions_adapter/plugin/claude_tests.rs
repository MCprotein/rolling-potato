use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn enabled_instruction_only_claude_skill_resolves() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_claude_plugin("safe-skill", "safe-plugin");
    let skill_dir = root.join("skills/hello");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\ndescription: 저장소를 읽기 전용으로 설명한다.\n---\n근거 파일을 확인하고 한국어로 설명하세요.\n",
    )
    .unwrap();

    import_report(PluginSource::ClaudeCode, root.to_str().unwrap(), false).unwrap();
    validate_report("imported.claude-code.safe-plugin").unwrap();
    set_enabled_report("imported.claude-code.safe-plugin", true).unwrap();

    let skill = resolve_imported_skill("imported.claude-code.safe-plugin.hello")
        .unwrap()
        .unwrap();
    assert_eq!(skill.plugin_id, "imported.claude-code.safe-plugin");
    assert_eq!(skill.source_path, "skills/hello/SKILL.md");
    assert_eq!(
        skill.instructions,
        "근거 파일을 확인하고 한국어로 설명하세요."
    );
    assert_eq!(skill.source_sha256.len(), 64);
    assert!(skill::list_report().contains("imported.claude-code.safe-plugin.hello"));

    cleanup_claude_plugin(root, data_root);
}

#[test]
fn enabled_instruction_only_claude_command_resolves() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_claude_plugin("safe-command", "command-plugin");
    fs::create_dir_all(root.join("commands")).unwrap();
    fs::write(
        root.join("commands/review.md"),
        "---\ndescription: 변경 내용을 읽기 전용으로 검토한다.\nargument-hint: [path]\nallowed-tools: Read Grep\n---\n요청된 경로의 변경 내용을 검토하세요.\n",
    )
    .unwrap();

    let imported = import_report(PluginSource::ClaudeCode, root.to_str().unwrap(), false).unwrap();
    assert!(imported.contains("claude-frontmatter:commands/review.md:argument-hint"));
    assert!(imported.contains("claude-frontmatter:commands/review.md:allowed-tools"));
    validate_report("imported.claude-code.command-plugin").unwrap();
    set_enabled_report("imported.claude-code.command-plugin", true).unwrap();

    let command = resolve_imported_skill("imported.claude-code.command-plugin.review")
        .unwrap()
        .unwrap();
    assert_eq!(command.plugin_id, "imported.claude-code.command-plugin");
    assert_eq!(command.source_path, "commands/review.md");
    assert_eq!(
        command.instructions,
        "요청된 경로의 변경 내용을 검토하세요."
    );

    cleanup_claude_plugin(root, data_root);
}

#[test]
fn claude_dynamic_shell_instruction_is_blocked_by_default() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let (root, data_root) = prepare_claude_plugin("dynamic-shell", "shell-plugin");
    fs::create_dir_all(root.join("commands")).unwrap();
    fs::write(
        root.join("commands/status.md"),
        "---\ndescription: git 상태를 확인한다.\n---\n!`git status --short`\n결과를 요약하세요.\n",
    )
    .unwrap();

    let imported = import_report(PluginSource::ClaudeCode, root.to_str().unwrap(), false).unwrap();
    assert!(imported
        .contains("command:commands/status.md (blocked-by-default, permission: shell-command)"));
    assert!(imported.contains("claude-dynamic-shell:commands/status.md"));
    assert!(imported.contains("blocked permissions: shell-command"));
    set_enabled_report("imported.claude-code.shell-plugin", true).unwrap();

    let error = resolve_imported_skill("imported.claude-code.shell-plugin.status").unwrap_err();
    assert_eq!(error.code, 3);
    assert!(error.message.contains("canonical instruction-only"));

    cleanup_claude_plugin(root, data_root);
}

#[test]
fn claude_unmapped_runtime_semantics_are_reported_explicitly() {
    let root = test_plugin_root("unsupported-semantics");
    fs::create_dir_all(root.join(".claude-plugin")).unwrap();
    fs::create_dir_all(root.join("agents")).unwrap();
    fs::create_dir_all(root.join("hooks")).unwrap();
    fs::create_dir_all(root.join("output-styles")).unwrap();
    fs::write(
        root.join(".claude-plugin/plugin.json"),
        r#"{
  "name":"semantic-plugin",
  "version":"1.0.0",
  "description":"semantic",
  "commands":"./custom/commands/",
  "agents":"./custom/agents/",
  "hooks":"./hooks/hooks.json",
  "mcpServers":"./.mcp.json",
  "lspServers":"./.lsp.json",
  "userConfig":{"token":{"type":"string","sensitive":true}},
  "channels":[],
  "dependencies":["helper"]
}"#,
    )
    .unwrap();
    fs::write(
        root.join("agents/reviewer.md"),
        "---\nname: reviewer\ndescription: review\n---\nReview.\n",
    )
    .unwrap();
    fs::write(root.join("hooks/hooks.json"), "{\"hooks\":{}}\n").unwrap();
    fs::write(root.join("output-styles/compact.md"), "# Compact\n").unwrap();
    fs::write(root.join(".mcp.json"), "{}\n").unwrap();
    fs::write(root.join(".lsp.json"), "{}\n").unwrap();
    fs::write(
        root.join("SKILL.md"),
        "---\nname: root-skill\ndescription: root\n---\nRoot.\n",
    )
    .unwrap();

    let report = import_report(PluginSource::ClaudeCode, root.to_str().unwrap(), true).unwrap();

    for unsupported in [
        "claude-manifest-custom-commands",
        "claude-manifest-custom-agents",
        "claude-manifest-hooks",
        "claude-manifest-mcp-servers",
        "claude-manifest-lsp-servers",
        "claude-manifest-user-config",
        "claude-manifest-channels",
        "claude-manifest-dependencies",
        "claude-subagent-semantics",
        "claude-hook-semantics",
        "claude-output-style-semantics",
        "claude-lsp-semantics",
        "claude-mcp-semantics",
        "claude-root-skill-layout",
    ] {
        assert!(
            report.contains(unsupported),
            "missing unsupported semantic: {unsupported}\n{report}"
        );
    }
    assert!(report.contains("subagent:agents/reviewer.md (unsupported"));
    assert!(report.contains("mcp-server"));
    assert!(report.contains("lsp-server"));
    assert!(report.contains("sensitive-config"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn claude_custom_command_paths_do_not_admit_default_commands() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let data_root = test_plugin_root("custom-command-data");
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var(
        "RPOTATO_PROJECT_ROOT",
        test_plugin_root("custom-command-project"),
    );
    let root = test_plugin_root("custom-command");
    fs::create_dir_all(root.join(".claude-plugin")).unwrap();
    fs::create_dir_all(root.join("commands")).unwrap();
    fs::create_dir_all(root.join("custom")).unwrap();
    fs::write(
        root.join(".claude-plugin/plugin.json"),
        r#"{"name":"custom-command-plugin","commands":"./custom/review.md"}"#,
    )
    .unwrap();
    fs::write(
        root.join("commands/review.md"),
        "---\ndescription: default command\n---\nDefault command.\n",
    )
    .unwrap();
    fs::write(
        root.join("custom/review.md"),
        "---\ndescription: custom command\n---\nCustom command.\n",
    )
    .unwrap();

    let report = import_report(PluginSource::ClaudeCode, root.to_str().unwrap(), false).unwrap();
    assert!(report.contains("claude-manifest-custom-commands"));
    set_enabled_report("imported.claude-code.custom-command-plugin", true).unwrap();
    assert!(!skill::list_report().contains("imported.claude-code.custom-command-plugin.review"));
    let error =
        resolve_imported_skill("imported.claude-code.custom-command-plugin.review").unwrap_err();
    assert!(error.message.contains("canonical instruction-only"));

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(data_root).unwrap();
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
}

fn prepare_claude_plugin(label: &str, plugin_name: &str) -> (PathBuf, PathBuf) {
    let data_root = test_plugin_root(&format!("{label}-data"));
    let project_root = test_plugin_root(&format!("{label}-project"));
    std::env::set_var("RPOTATO_DATA_HOME", &data_root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
    let root = test_plugin_root(label);
    fs::create_dir_all(root.join(".claude-plugin")).unwrap();
    fs::write(
        root.join(".claude-plugin/plugin.json"),
        format!(r#"{{"name":"{plugin_name}","version":"1.0.0","description":"test"}}"#),
    )
    .unwrap();
    (root, data_root)
}

fn cleanup_claude_plugin(root: PathBuf, data_root: PathBuf) {
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(data_root).unwrap();
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
}

fn test_plugin_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rpotato-claude-plugin-test-{}-{}-{unique}",
        std::process::id(),
        label
    ))
}
