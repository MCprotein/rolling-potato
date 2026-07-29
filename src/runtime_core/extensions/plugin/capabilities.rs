use super::extract_json_string_array;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginCapability {
    pub(crate) kind: String,
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) required_permission: String,
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
