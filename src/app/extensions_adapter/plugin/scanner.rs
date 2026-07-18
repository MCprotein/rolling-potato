use std::fs;
use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::extensions::plugin::{
    finalize_permissions, finalize_unsupported, is_unsupported_plugin_asset, push_capability,
    push_unique, push_unsupported_capability, PluginCapability,
};
use crate::surfaces::cli::command::PluginSource;

const ENTRY_LIMIT: usize = 10_000;

#[derive(Clone, Default)]
pub(super) struct DirectoryScan {
    pub(super) files: usize,
    pub(super) directories: usize,
    pub(super) capabilities: Vec<PluginCapability>,
    pub(super) required_permissions: Vec<String>,
    pub(super) unsupported: Vec<String>,
}

pub(super) fn scan_directory(root: &Path, source: PluginSource) -> Result<DirectoryScan, AppError> {
    let mut scan = DirectoryScan::default();
    let mut stack = vec![root.to_path_buf()];

    while let Some(path) = stack.pop() {
        let entries = fs::read_dir(&path).map_err(|err| {
            AppError::usage(format!(
                "plugin directory를 읽을 수 없습니다: {}\n이유: {err}",
                path.display()
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|err| {
                AppError::usage(format!(
                    "plugin directory entry를 읽을 수 없습니다: {}\n이유: {err}",
                    path.display()
                ))
            })?;
            let file_type = entry.file_type().map_err(|err| {
                AppError::usage(format!(
                    "plugin path type을 확인할 수 없습니다: {}\n이유: {err}",
                    entry.path().display()
                ))
            })?;

            if file_type.is_symlink() {
                return Err(AppError::blocked(format!(
                    "plugin directory 안의 symlink는 boundary 우회 위험 때문에 차단합니다: {}",
                    entry.path().display()
                )));
            }

            let file_name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            let entry_path = entry.path();
            let relative_path = relative_plugin_path(root, &entry_path);
            if file_type.is_dir() {
                if file_name == "bin" {
                    push_permission_and_capability(
                        &mut scan,
                        "bin",
                        &relative_path,
                        "bin-executable",
                    );
                }
                if file_name == "mcp" || file_name == "mcp-servers" {
                    push_permission_and_capability(
                        &mut scan,
                        "mcp-server",
                        &relative_path,
                        "mcp-server",
                    );
                }
                if file_name == "hooks" {
                    push_permission_and_capability(&mut scan, "hook", &relative_path, "hook");
                }
                if source == PluginSource::Codex && file_name == "skills" {
                    push_capability(&mut scan.capabilities, "skill", &relative_path, "none");
                }
                if file_name == "agents" {
                    if source == PluginSource::ClaudeCode {
                        push_unsupported_capability(
                            &mut scan.capabilities,
                            "subagent",
                            &relative_path,
                        );
                        push_unique(&mut scan.unsupported, "claude-subagent-semantics");
                    } else {
                        push_capability(&mut scan.capabilities, "subagent", &relative_path, "none");
                    }
                }
                if file_name == "lsp" {
                    push_permission_and_capability(
                        &mut scan,
                        "lsp-server",
                        &relative_path,
                        "lsp-server",
                    );
                }
                if file_name == "monitors" || file_name == "monitor" {
                    push_permission_and_capability(
                        &mut scan,
                        "monitor",
                        &relative_path,
                        "background-process",
                    );
                }
                if file_name == "settings" {
                    push_permission_and_capability(
                        &mut scan,
                        "runtime-settings",
                        &relative_path,
                        "runtime-settings",
                    );
                }
                if source == PluginSource::ClaudeCode {
                    super::claude::record_directory_semantics(&file_name, &mut scan);
                }
                scan.directories += 1;
                stack.push(entry_path);
            } else if file_type.is_file() {
                if file_name.ends_with(".sh")
                    || file_name.ends_with(".ps1")
                    || file_name.ends_with(".bat")
                    || file_name.ends_with(".cmd")
                {
                    push_permission_and_capability(
                        &mut scan,
                        "shell-command",
                        &relative_path,
                        "shell-command",
                    );
                }
                if file_name.contains("background") || file_name.contains("monitor") {
                    push_permission_and_capability(
                        &mut scan,
                        "background-process",
                        &relative_path,
                        "background-process",
                    );
                }
                if file_name == "settings.json" {
                    push_permission_and_capability(
                        &mut scan,
                        "runtime-settings",
                        &relative_path,
                        "runtime-settings",
                    );
                }
                if file_name == ".env"
                    || file_name.contains("secret")
                    || file_name.contains("token")
                    || file_name.contains("credential")
                {
                    push_permission_and_capability(
                        &mut scan,
                        "sensitive-config",
                        &relative_path,
                        "sensitive-config",
                    );
                }
                classify_runtime_file(source, &entry_path, &relative_path, &mut scan)?;
                if is_unsupported_plugin_asset(&relative_path) {
                    push_unique(
                        &mut scan.unsupported,
                        "runtime-specific-asset-review-required",
                    );
                }
                scan.files += 1;
            }

            if scan.files + scan.directories > ENTRY_LIMIT {
                return Err(AppError::blocked(format!(
                    "plugin directory entry 수가 너무 많습니다. 현재 제한: {ENTRY_LIMIT}"
                )));
            }
        }
    }

    finalize_permissions(&mut scan.required_permissions);
    finalize_unsupported(&mut scan.unsupported);

    Ok(scan)
}

pub(super) fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), AppError> {
    fs::create_dir_all(destination).map_err(|err| {
        AppError::runtime(format!(
            "plugin source snapshot directory를 만들지 못했습니다: {} ({err})",
            destination.display()
        ))
    })?;

    for entry in fs::read_dir(source).map_err(|err| {
        AppError::usage(format!(
            "plugin source directory를 읽지 못했습니다: {} ({err})",
            source.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::usage(format!(
                "plugin source entry를 읽지 못했습니다: {} ({err})",
                source.display()
            ))
        })?;
        let kind = entry.file_type().map_err(|err| {
            AppError::usage(format!(
                "plugin source entry type을 확인하지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;
        let target = destination.join(entry.file_name());

        if kind.is_symlink() {
            return Err(AppError::blocked(format!(
                "plugin source symlink는 snapshot 대상에서 차단합니다: {}",
                entry.path().display()
            )));
        }
        if kind.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target).map_err(|err| {
                AppError::runtime(format!(
                    "plugin source file을 복사하지 못했습니다: {} -> {} ({err})",
                    entry.path().display(),
                    target.display()
                ))
            })?;
        }
    }

    Ok(())
}

fn push_permission_and_capability(
    scan: &mut DirectoryScan,
    kind: &str,
    path: &str,
    required_permission: &str,
) {
    push_unique(&mut scan.required_permissions, required_permission);
    push_capability(&mut scan.capabilities, kind, path, required_permission);
}

fn classify_runtime_file(
    source: PluginSource,
    absolute_path: &Path,
    relative_path: &str,
    scan: &mut DirectoryScan,
) -> Result<(), AppError> {
    let lower = relative_path.to_ascii_lowercase();

    if lower == ".codex-plugin/plugin.json" || lower == ".claude-plugin/plugin.json" {
        push_capability(&mut scan.capabilities, "manifest", relative_path, "none");
    }

    if lower.starts_with("bin/") {
        push_permission_and_capability(scan, "bin", relative_path, "bin-executable");
    }
    if lower.starts_with("mcp/") || lower.starts_with("mcp-servers/") {
        push_permission_and_capability(scan, "mcp-server", relative_path, "mcp-server");
    }
    if lower.starts_with("hooks/") {
        push_permission_and_capability(scan, "hook", relative_path, "hook");
        if source == PluginSource::ClaudeCode {
            push_unique(&mut scan.unsupported, "claude-hook-semantics");
        }
    }
    if lower.starts_with("apps/") || lower.starts_with("app-integrations/") {
        push_permission_and_capability(scan, "app-integration", relative_path, "remote-connector");
    }
    if lower == ".mcp.json" {
        push_permission_and_capability(scan, "mcp-server", relative_path, "mcp-server");
        if source == PluginSource::ClaudeCode {
            push_unique(&mut scan.unsupported, "claude-mcp-semantics");
        }
    }
    if lower == ".app.json" {
        push_permission_and_capability(scan, "app-integration", relative_path, "remote-connector");
    }

    match source {
        PluginSource::Codex => {
            if lower.starts_with("skills/") && lower.ends_with("skill.md") {
                push_capability(&mut scan.capabilities, "skill", relative_path, "none");
            }
            if lower.starts_with("skills/") && lower.contains("/scripts/") {
                push_permission_and_capability(scan, "skill-script", relative_path, "skill-script");
            }
        }
        PluginSource::ClaudeCode => {
            super::claude::classify_file(absolute_path, relative_path, scan)?;
        }
    }
    Ok(())
}

fn relative_plugin_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            Component::CurDir => None,
            other => Some(other.as_os_str().to_string_lossy().to_string()),
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn sha256_directory_snapshot(root: &Path) -> Result<String, AppError> {
    let mut entries = Vec::new();
    collect_snapshot_entries(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));

    let mut hasher = Sha256::new();
    for (kind, path, hash) in entries {
        hasher.update(kind.as_bytes());
        hasher.update(b"\0");
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    Ok(sha256_bytes_to_hex(&digest))
}

fn collect_snapshot_entries(
    root: &Path,
    current: &Path,
    entries: &mut Vec<(String, String, String)>,
) -> Result<(), AppError> {
    for entry in fs::read_dir(current).map_err(|err| {
        AppError::usage(format!(
            "plugin snapshot directory를 읽지 못했습니다: {} ({err})",
            current.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::usage(format!(
                "plugin snapshot entry를 읽지 못했습니다: {} ({err})",
                current.display()
            ))
        })?;
        let file_type = entry.file_type().map_err(|err| {
            AppError::usage(format!(
                "plugin snapshot path type을 확인하지 못했습니다: {} ({err})",
                entry.path().display()
            ))
        })?;
        if file_type.is_symlink() {
            return Err(AppError::blocked(format!(
                "plugin snapshot symlink는 hash 대상에서 차단합니다: {}",
                entry.path().display()
            )));
        }

        let entry_path = entry.path();
        let relative_path = relative_plugin_path(root, &entry_path);
        if file_type.is_dir() {
            entries.push(("dir".to_string(), relative_path, String::new()));
            collect_snapshot_entries(root, &entry_path, entries)?;
        } else if file_type.is_file() {
            let file_hash = checksum::sha256_file(&entry_path)?;
            entries.push(("file".to_string(), relative_path, file_hash));
        }

        if entries.len() > ENTRY_LIMIT {
            return Err(AppError::blocked(format!(
                "plugin snapshot entry 수가 너무 많습니다. 현재 제한: {ENTRY_LIMIT}"
            )));
        }
    }

    Ok(())
}

fn sha256_bytes_to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
