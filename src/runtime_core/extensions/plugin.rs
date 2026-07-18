//! Plugin manifest parsing and default-deny capability policy.

use crate::foundation::error::AppError;
use std::path::{Component, Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginCapability {
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) required_permission: String,
}

pub(crate) struct ParsedCodexSkill {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) instructions: String,
}

pub(crate) struct ParsedClaudeInstruction {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) instructions: String,
}

pub(crate) fn parse_codex_skill(text: &str, path: &Path) -> Result<ParsedCodexSkill, AppError> {
    let normalized = text.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: SKILL.md YAML frontmatter가 없습니다.",
            path.display()
        )));
    };
    let Some((frontmatter, instructions)) = rest.split_once("\n---\n") else {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: SKILL.md YAML frontmatter 종료 marker가 없습니다.",
            path.display()
        )));
    };
    let field = |name: &str| {
        frontmatter
            .lines()
            .find_map(|line| line.split_once(':').filter(|(key, _)| key.trim() == name))
            .map(|(_, value)| unquote_yaml_scalar(value.trim()))
            .filter(|value| !value.is_empty())
    };
    let name = field("name").ok_or_else(|| {
        AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: frontmatter name이 없습니다.",
            path.display()
        ))
    })?;
    validate_component_name(&name, "skill")?;
    let description = field("description").ok_or_else(|| {
        AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: frontmatter description이 없습니다.",
            path.display()
        ))
    })?;
    let instructions = instructions.trim().to_string();
    if instructions.is_empty() {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- path: {}\n- 이유: instruction body가 비어 있습니다.",
            path.display()
        )));
    }
    Ok(ParsedCodexSkill {
        name,
        description,
        instructions,
    })
}

pub(crate) fn parse_claude_instruction(
    text: &str,
    path: &Path,
    invocation_name: &str,
) -> Result<ParsedClaudeInstruction, AppError> {
    validate_component_name(invocation_name, "skill")?;
    if contains_claude_dynamic_shell(text) {
        return Err(AppError::blocked(format!(
            "plugin instruction 실행 차단\n- path: {}\n- 이유: Claude Code 동적 shell 삽입은 rpotato read-only adapter에서 지원하지 않습니다.",
            path.display()
        )));
    }
    let normalized = text.replace("\r\n", "\n");
    let (frontmatter, body) = split_optional_frontmatter(&normalized, path)?;
    let description = yaml_scalar_field(frontmatter, "description")
        .or_else(|| first_markdown_paragraph(body))
        .ok_or_else(|| {
            AppError::blocked(format!(
                "plugin instruction 실행 차단\n- path: {}\n- 이유: description 또는 instruction paragraph가 없습니다.",
                path.display()
            ))
        })?;
    let instructions = body.trim().to_string();
    if instructions.is_empty() {
        return Err(AppError::blocked(format!(
            "plugin instruction 실행 차단\n- path: {}\n- 이유: instruction body가 비어 있습니다.",
            path.display()
        )));
    }
    Ok(ParsedClaudeInstruction {
        name: invocation_name.to_string(),
        description,
        instructions,
    })
}

pub(crate) fn contains_claude_dynamic_shell(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("!`") || trimmed.starts_with("```!")
    })
}

pub(crate) fn claude_instruction_unsupported(text: &str, relative_path: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    let frontmatter = normalized
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(frontmatter, _)| frontmatter)
        .unwrap_or_default();
    let mut unsupported = Vec::new();
    for field in [
        "when_to_use",
        "argument-hint",
        "arguments",
        "disable-model-invocation",
        "user-invocable",
        "allowed-tools",
        "disallowed-tools",
        "model",
        "effort",
        "context",
        "agent",
        "hooks",
        "paths",
        "shell",
    ] {
        if yaml_has_field(frontmatter, field) {
            push_unique(
                &mut unsupported,
                &format!("claude-frontmatter:{relative_path}:{field}"),
            );
        }
    }
    if normalized.contains("$ARGUMENTS")
        || normalized.contains("${CLAUDE_PLUGIN_ROOT}")
        || normalized.contains("${CLAUDE_PLUGIN_DATA}")
        || normalized.contains("${CLAUDE_PROJECT_DIR}")
        || normalized.contains("${user_config.")
    {
        push_unique(
            &mut unsupported,
            &format!("claude-template-substitution:{relative_path}"),
        );
    }
    unsupported
}

fn split_optional_frontmatter<'a>(
    text: &'a str,
    path: &Path,
) -> Result<(&'a str, &'a str), AppError> {
    let Some(rest) = text.strip_prefix("---\n") else {
        return Ok(("", text));
    };
    rest.split_once("\n---\n").ok_or_else(|| {
        AppError::blocked(format!(
            "plugin instruction 실행 차단\n- path: {}\n- 이유: YAML frontmatter 종료 marker가 없습니다.",
            path.display()
        ))
    })
}

fn yaml_scalar_field(frontmatter: &str, name: &str) -> Option<String> {
    frontmatter
        .lines()
        .find_map(|line| line.split_once(':').filter(|(key, _)| key.trim() == name))
        .map(|(_, value)| unquote_yaml_scalar(value.trim()))
        .filter(|value| !value.is_empty() && value != "|" && value != ">")
}

fn yaml_has_field(frontmatter: &str, name: &str) -> bool {
    frontmatter.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(key, _)| key.trim() == name)
    })
}

fn first_markdown_paragraph(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.chars().take(240).collect())
}

pub(crate) fn unquote_yaml_scalar(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

pub(crate) fn validate_component_name(value: &str, kind: &str) -> Result<(), AppError> {
    let valid = !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-');
    if valid {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "plugin {kind} name 형식이 올바르지 않습니다: {value}"
        )))
    }
}

pub(crate) fn reject_remote_or_marketplace(raw_path: &str) -> Result<(), AppError> {
    let lower = raw_path.to_ascii_lowercase();
    let rejected_prefixes = [
        "http://",
        "https://",
        "ssh://",
        "git://",
        "git@",
        "marketplace:",
        "registry:",
        "catalog:",
    ];

    if rejected_prefixes
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        || lower.contains("://")
    {
        return Err(AppError::blocked(format!(
            "plugin import는 local directory만 허용합니다. remote URL, marketplace, registry, catalog source는 지원하지 않습니다: {raw_path}"
        )));
    }

    Ok(())
}

pub(crate) fn reject_path_traversal(raw_path: &str) -> Result<(), AppError> {
    if Path::new(raw_path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::blocked(
            "plugin import path에는 상위 경로(..)를 포함할 수 없습니다.",
        ));
    }

    Ok(())
}

pub(crate) fn validate_plugin_id(id: &str) -> Result<(), AppError> {
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_')
    {
        return Err(AppError::usage(format!(
            "plugin id 형식이 올바르지 않습니다: {id}"
        )));
    }
    Ok(())
}

pub(crate) fn extract_json_string_field(text: &str, field: &str) -> Option<String> {
    let quoted_field = format!("\"{field}\"");
    let field_index = text.find(&quoted_field)?;
    let after_field = &text[field_index + quoted_field.len()..];
    let colon_index = after_field.find(':')?;
    let after_colon = after_field[colon_index + 1..].trim_start();
    let value = after_colon.strip_prefix('"')?;

    let mut escaped = false;
    let mut result = String::new();
    for ch in value.chars() {
        if escaped {
            result.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(result),
            _ => result.push(ch),
        }
    }

    None
}

pub(crate) fn extract_json_string_array(text: &str, field: &str) -> Vec<String> {
    let quoted_field = format!("\"{field}\"");
    let Some(field_index) = text.find(&quoted_field) else {
        return Vec::new();
    };
    let after_field = &text[field_index + quoted_field.len()..];
    let Some(colon_index) = after_field.find(':') else {
        return Vec::new();
    };
    let after_colon = after_field[colon_index + 1..].trim_start();
    let Some(array_body) = after_colon.strip_prefix('[') else {
        return Vec::new();
    };
    let Some(end) = array_body.find(']') else {
        return Vec::new();
    };

    array_body[..end]
        .split(',')
        .filter_map(|item| {
            let trimmed = item.trim();
            trimmed
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .map(|value| value.replace("\\\"", "\""))
        })
        .collect()
}

pub(crate) fn required_field(text: &str, field: &str) -> Result<String, AppError> {
    extract_json_string_field(text, field).ok_or_else(|| {
        AppError::usage(format!(
            "normalized plugin manifest에 필수 field가 없습니다: {field}"
        ))
    })
}

pub(crate) fn required_usize(text: &str, field: &str) -> Result<usize, AppError> {
    let quoted_field = format!("\"{field}\":");
    let start = text.find(&quoted_field).ok_or_else(|| {
        AppError::usage(format!(
            "normalized plugin manifest에 필수 number field가 없습니다: {field}"
        ))
    })? + quoted_field.len();
    let value = text[start..]
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    value.parse::<usize>().map_err(|_| {
        AppError::usage(format!(
            "normalized plugin manifest number field가 올바르지 않습니다: {field}"
        ))
    })
}

pub(crate) fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch == '-' || ch == '_' || ch == '.' || ch.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if slug.is_empty() {
        "plugin".to_string()
    } else {
        slug
    }
}

pub(crate) fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

pub(crate) fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

impl PluginCapability {
    pub(crate) fn new(kind: &str, path: &str, status: &str, required_permission: &str) -> Self {
        Self {
            kind: kind.to_string(),
            path: path.to_string(),
            status: status.to_string(),
            required_permission: required_permission.to_string(),
        }
    }

    pub(crate) fn summary(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.kind, self.path, self.status, self.required_permission
        )
    }

    pub(crate) fn from_summary(value: &str) -> Option<Self> {
        let mut parts = value.splitn(4, '|');
        Some(Self {
            kind: parts.next()?.to_string(),
            path: parts.next()?.to_string(),
            status: parts.next()?.to_string(),
            required_permission: parts.next()?.to_string(),
        })
    }
}

pub(crate) fn push_capability(
    capabilities: &mut Vec<PluginCapability>,
    kind: &str,
    path: &str,
    required_permission: &str,
) {
    let status = if required_permission == "none" {
        "mapped"
    } else {
        "blocked-by-default"
    };
    let capability = PluginCapability::new(kind, path, status, required_permission);
    if !capabilities.iter().any(|existing| existing == &capability) {
        capabilities.push(capability);
    }
}

pub(crate) fn push_unsupported_capability(
    capabilities: &mut Vec<PluginCapability>,
    kind: &str,
    path: &str,
) {
    let capability = PluginCapability::new(kind, path, "unsupported", "unsupported");
    if !capabilities.iter().any(|existing| existing == &capability) {
        capabilities.push(capability);
    }
}

pub(crate) fn is_unsupported_plugin_asset(relative_path: &str) -> bool {
    let lower = relative_path.to_ascii_lowercase();
    lower.starts_with("marketplace/")
        || lower.contains("/marketplace/")
        || lower.starts_with("registry/")
        || lower.contains("/registry/")
        || lower.ends_with(".vsix")
}

pub(crate) fn apply_manifest_risk_markers(
    manifest_text: &str,
    required_permissions: &mut Vec<String>,
) {
    let lower = manifest_text.to_ascii_lowercase();
    if lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("git@")
        || lower.contains("://")
    {
        push_unique(required_permissions, "remote-connector");
    }
    if lower.contains("\"mcp\"")
        || lower.contains("\"mcpservers\"")
        || lower.contains("\"mcp_servers\"")
    {
        push_unique(required_permissions, "mcp-server");
    }
    if lower.contains("background") || lower.contains("\"monitor\"") {
        push_unique(required_permissions, "background-process");
    }
    if lower.contains("file_write") || lower.contains("filewrite") || lower.contains("\"write\"") {
        push_unique(required_permissions, "file-write");
    }
    if lower.contains("apikey")
        || lower.contains("api_key")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("password")
    {
        push_unique(required_permissions, "sensitive-config");
    }
}

pub(crate) fn apply_claude_manifest_semantics(
    manifest_text: &str,
    required_permissions: &mut Vec<String>,
    unsupported: &mut Vec<String>,
) {
    for (field, label) in [
        ("skills", "claude-manifest-custom-skills"),
        ("commands", "claude-manifest-custom-commands"),
        ("agents", "claude-manifest-custom-agents"),
        ("hooks", "claude-manifest-hooks"),
        ("mcpServers", "claude-manifest-mcp-servers"),
        ("outputStyles", "claude-manifest-output-styles"),
        ("lspServers", "claude-manifest-lsp-servers"),
        ("userConfig", "claude-manifest-user-config"),
        ("channels", "claude-manifest-channels"),
        ("dependencies", "claude-manifest-dependencies"),
        ("defaultEnabled", "claude-manifest-default-enablement"),
        ("experimental", "claude-manifest-experimental-components"),
    ] {
        if json_has_field(manifest_text, field) {
            push_unique(unsupported, label);
        }
    }
    if json_has_field(manifest_text, "userConfig") {
        push_unique(required_permissions, "runtime-settings");
    }
    if json_has_field(manifest_text, "channels") || json_has_field(manifest_text, "dependencies") {
        push_unique(required_permissions, "remote-connector");
    }
}

fn json_has_field(text: &str, field: &str) -> bool {
    text.contains(&format!("\"{field}\""))
}

pub(crate) fn finalize_permissions(required_permissions: &mut Vec<String>) {
    if required_permissions.is_empty() {
        required_permissions.push("none".to_string());
        return;
    }

    required_permissions.sort();
    required_permissions.dedup();
    if required_permissions.len() > 1 {
        required_permissions.retain(|permission| permission != "none");
    }
}

pub(crate) fn finalize_unsupported(unsupported: &mut Vec<String>) {
    if unsupported.is_empty() {
        unsupported.push("none".to_string());
        return;
    }
    unsupported.sort();
    unsupported.dedup();
    if unsupported.len() > 1 {
        unsupported.retain(|value| value != "none");
    }
}

pub(crate) fn blocked_permissions(required_permissions: &[String]) -> Vec<String> {
    let mut blocked = required_permissions
        .iter()
        .filter(|permission| permission.as_str() != "none")
        .cloned()
        .collect::<Vec<_>>();
    blocked.sort();
    blocked.dedup();
    if blocked.is_empty() {
        blocked.push("none".to_string());
    }
    blocked
}

pub(crate) fn blocked_permissions_from_json(text: &str) -> Vec<String> {
    let blocked = extract_json_string_array(text, "blockedPermissions");
    if blocked.is_empty() {
        blocked_permissions(&extract_json_string_array(text, "requiredPermissions"))
    } else {
        blocked
    }
}

pub(crate) fn capability_summary(capabilities: &[PluginCapability]) -> Vec<String> {
    capabilities.iter().map(PluginCapability::summary).collect()
}

pub(crate) fn capability_summary_from_json(text: &str) -> Vec<PluginCapability> {
    extract_json_string_array(text, "capabilitySummary")
        .iter()
        .filter_map(|summary| PluginCapability::from_summary(summary))
        .collect()
}

pub(crate) fn display_capabilities(capabilities: &[PluginCapability]) -> String {
    if capabilities.is_empty() {
        return "none".to_string();
    }

    capabilities
        .iter()
        .map(|capability| {
            format!(
                "{}:{} ({}, permission: {})",
                capability.kind, capability.path, capability.status, capability.required_permission
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}
