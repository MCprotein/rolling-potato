use super::scanner::DirectoryScan;
use crate::foundation::error::AppError;
use crate::runtime_core::extensions::plugin::{
    claude_instruction_unsupported, contains_claude_dynamic_shell, push_capability, push_unique,
    push_unsupported_capability,
};

use std::fs;
use std::path::Path;

pub(super) fn classify_file(
    absolute_path: &Path,
    relative_path: &str,
    scan: &mut DirectoryScan,
) -> Result<(), AppError> {
    let lower = relative_path.to_ascii_lowercase();
    if lower == "skill.md" {
        push_unsupported_capability(&mut scan.capabilities, "skill", relative_path);
        push_unique(&mut scan.unsupported, "claude-root-skill-layout");
    }
    if is_canonical_skill(&lower) {
        classify_instruction("skill", absolute_path, relative_path, scan)?;
    }
    if lower.starts_with("skills/") && lower.contains("/scripts/") {
        push_permission_capability(scan, "skill-script", relative_path, "skill-script");
    }
    if lower.starts_with("agents/") {
        push_unsupported_capability(&mut scan.capabilities, "subagent", relative_path);
        push_unique(&mut scan.unsupported, "claude-subagent-semantics");
    }
    if is_canonical_command(&lower) {
        classify_instruction("command", absolute_path, relative_path, scan)?;
    } else if lower.starts_with("commands/") {
        push_unsupported_capability(&mut scan.capabilities, "command", relative_path);
        push_unique(&mut scan.unsupported, "claude-command-layout");
    }
    if lower == ".lsp.json" || lower.starts_with("lsp/") {
        push_permission_capability(scan, "lsp-server", relative_path, "lsp-server");
        push_unique(&mut scan.unsupported, "claude-lsp-semantics");
    }
    if lower.starts_with("monitors/") || lower.starts_with("monitor/") {
        push_permission_capability(scan, "monitor", relative_path, "background-process");
        push_unique(&mut scan.unsupported, "claude-monitor-semantics");
    }
    if lower == "settings.json" || lower.starts_with("settings/") {
        push_permission_capability(scan, "runtime-settings", relative_path, "runtime-settings");
        push_unique(&mut scan.unsupported, "claude-settings-semantics");
    }
    if lower.starts_with("bin/") {
        push_unique(&mut scan.unsupported, "claude-bin-path-semantics");
    }
    if lower.starts_with("output-styles/") {
        push_unsupported_capability(&mut scan.capabilities, "output-style", relative_path);
        push_unique(&mut scan.unsupported, "claude-output-style-semantics");
    }
    if lower.starts_with("themes/") {
        push_unsupported_capability(&mut scan.capabilities, "theme", relative_path);
        push_unique(&mut scan.unsupported, "claude-theme-semantics");
    }
    Ok(())
}

pub(super) fn record_directory_semantics(file_name: &str, scan: &mut DirectoryScan) {
    let unsupported = match file_name {
        "hooks" => Some("claude-hook-semantics"),
        "agents" => Some("claude-subagent-semantics"),
        "lsp" => Some("claude-lsp-semantics"),
        "monitors" | "monitor" => Some("claude-monitor-semantics"),
        "bin" => Some("claude-bin-path-semantics"),
        "settings" => Some("claude-settings-semantics"),
        "output-styles" => Some("claude-output-style-semantics"),
        "themes" => Some("claude-theme-semantics"),
        _ => None,
    };
    if let Some(value) = unsupported {
        push_unique(&mut scan.unsupported, value);
    }
}

fn classify_instruction(
    kind: &str,
    absolute_path: &Path,
    relative_path: &str,
    scan: &mut DirectoryScan,
) -> Result<(), AppError> {
    let text = fs::read_to_string(absolute_path).map_err(|err| {
        AppError::usage(format!(
            "Claude Code plugin instruction을 읽을 수 없습니다: {} ({err})",
            absolute_path.display()
        ))
    })?;
    for unsupported in claude_instruction_unsupported(&text, relative_path) {
        push_unique(&mut scan.unsupported, &unsupported);
    }
    if contains_claude_dynamic_shell(&text) {
        push_permission_capability(scan, kind, relative_path, "shell-command");
        push_unique(
            &mut scan.unsupported,
            &format!("claude-dynamic-shell:{relative_path}"),
        );
    } else {
        push_capability(&mut scan.capabilities, kind, relative_path, "none");
    }
    Ok(())
}

fn push_permission_capability(
    scan: &mut DirectoryScan,
    kind: &str,
    path: &str,
    required_permission: &str,
) {
    push_unique(&mut scan.required_permissions, required_permission);
    push_capability(&mut scan.capabilities, kind, path, required_permission);
}

fn is_canonical_skill(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    parts.len() == 3 && parts[0] == "skills" && parts[2] == "skill.md"
}

fn is_canonical_command(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    parts.len() == 2 && parts[0] == "commands" && parts[1].ends_with(".md")
}
