use super::skill;
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::extensions::plugin::*;
use crate::surfaces::cli::command::PluginSource;

use std::fs;
use std::path::{Path, PathBuf};

mod execution;
mod registry;
mod scanner;
mod source_import;

pub use execution::{resolve_imported_codex_skill, revalidate_completed_codex_skill};
use source_import::{inspect_source_plugin, normalize_plugin};

#[cfg(test)]
use registry::normalized_manifest_path;
use registry::{
    persist_plugin, plugin_data_path, plugin_dir, read_plugin, read_plugins,
    verify_imported_snapshot, write_plugin_manifest, write_validation_report, PluginSnapshot,
};
use scanner::{scan_directory, sha256_directory_snapshot, DirectoryScan};

const IMPORTED_SKILL_MAX_BYTES: u64 = 64 * 1024;
const PLUGIN_MANIFEST_SCHEMA_VERSION: usize = 2;
const PLUGIN_ADAPTER_VERSION: &str = "rpotato-plugin-adapter-v0.37.0";
const PLUGIN_PERMISSION_POLICY: &str = "default-deny-external-capabilities-v2";

pub fn list_report() -> String {
    let plugins = match read_plugins() {
        Ok(plugins) => plugins,
        Err(err) => {
            return format!(
                "plugin registry\n- 상태: registry 읽기 실패\n- 이유: {}\n- imported plugins dir: {}\n- marketplace, registry, catalog, remote URL import는 지원하지 않습니다.",
                err.message,
                paths::imported_plugins_dir().display()
            );
        }
    };

    if plugins.is_empty() {
        return format!(
            "plugin registry\n- 상태: imported plugin 없음\n- imported plugins dir: {}\n- plugin data dir: {}\n- source runtime namespace: native, codex, claude-code\n- marketplace, registry, catalog, remote URL import는 지원하지 않습니다.",
            paths::imported_plugins_dir().display(),
            paths::plugin_data_dir().display()
        );
    }

    let rows = plugins
        .iter()
        .map(|plugin| {
            format!(
                "- {} | source: {} | status: {} | name: {} | version: {}",
                plugin.id, plugin.source_label, plugin.status, plugin.name, plugin.version
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "plugin registry\n- imported plugins: {}\n- imported plugins dir: {}\n- plugin data dir: {}\n{}",
        plugins.len(),
        paths::imported_plugins_dir().display(),
        paths::plugin_data_dir().display(),
        rows
    )
}

pub fn enabled_codex_skill_rows() -> Vec<String> {
    let Ok(plugins) = read_plugins() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for plugin in plugins.into_iter().filter(|plugin| {
        plugin.source == PluginSource::Codex
            && plugin.status == "enabled"
            && plugin.adapter_version == PLUGIN_ADAPTER_VERSION
            && plugin.permission_policy == PLUGIN_PERMISSION_POLICY
    }) {
        let Some(plugin_name) = plugin.id.strip_prefix("imported.codex.") else {
            continue;
        };
        for capability in plugin.capabilities.iter().filter(|capability| {
            capability.kind == "skill"
                && capability.status == "mapped"
                && capability.required_permission == "none"
        }) {
            let Some(skill_name) = capability
                .path
                .strip_prefix("skills/")
                .and_then(|path| path.strip_suffix("/SKILL.md"))
            else {
                continue;
            };
            if validate_component_name(skill_name, "skill").is_err()
                || plugin.capabilities.iter().any(|candidate| {
                    candidate.required_permission != "none"
                        && Path::new(&candidate.path)
                            .starts_with(Path::new(&format!("skills/{skill_name}")))
                })
            {
                continue;
            }
            rows.push(format!(
                "- imported.codex.{}.{} | mode: read-only | 실행 시 snapshot/frontmatter 재검증",
                plugin_name, skill_name
            ));
        }
    }
    rows.sort();
    rows.dedup();
    rows
}

pub fn import_report(
    source: PluginSource,
    raw_path: &str,
    dry_run: bool,
) -> Result<String, AppError> {
    let source_plugin = inspect_source_plugin(source, raw_path)?;
    let plugin = normalize_plugin(source, raw_path, &source_plugin);
    let permission_summary = display_vec(&plugin.required_permissions);
    let blocked_summary = display_vec(&plugin.blocked_permissions);
    let unsupported_summary = display_vec(&plugin.unsupported);
    let capability_summary = display_capabilities(&plugin.capabilities);

    if dry_run {
        return Ok(format!(
            "plugin import 검사\n- source runtime: {}\n- plugin id: {}\n- adapter version: {}\n- permission policy: {}\n- local path: {}\n- manifest: {}\n- source manifest sha256: {}\n- source snapshot sha256: {}\n- name: {}\n- version: {}\n- description: {}\n- files: {}\n- directories: {}\n- capabilities: {}\n- required permissions: {}\n- blocked permissions: {}\n- unsupported: {}\n- remote source: 거부 정책 적용\n- marketplace/registry/catalog: 미지원\n- 실행 상태: dry-run: 파일 복사, registry 기록, capability enable을 수행하지 않았습니다.",
            source.label(),
            plugin.id,
            plugin.adapter_version,
            plugin.permission_policy,
            source_plugin.root.display(),
            source_plugin.manifest.display(),
            plugin.source_manifest_sha256,
            plugin.source_snapshot_sha256,
            plugin.name,
            plugin.version,
            plugin.description,
            plugin.files,
            plugin.directories,
            capability_summary,
            permission_summary,
            blocked_summary,
            unsupported_summary
        ));
    }

    persist_plugin(&plugin, &source_plugin.root)?;
    let event_id = state::record_event(
        "plugin.imported",
        "plugin local directory import 완료",
        &format!("plugin_id={} source={}", plugin.id, plugin.source_label),
    )?;

    Ok(format!(
        "plugin import 결과\n- source runtime: {}\n- plugin id: {}\n- adapter version: {}\n- permission policy: {}\n- imported path: {}\n- data path: {}\n- status: {}\n- source manifest sha256: {}\n- source snapshot sha256: {}\n- capabilities: {}\n- required permissions: {}\n- blocked permissions: {}\n- unsupported: {}\n- ledger event: {}\n- 동작: source snapshot과 normalized manifest를 저장했습니다. capability 실행 권한은 부여하지 않았습니다.",
        source.label(),
        plugin.id,
        plugin.adapter_version,
        plugin.permission_policy,
        plugin_dir(&plugin.id).display(),
        plugin_data_path(&plugin.id).display(),
        plugin.status,
        plugin.source_manifest_sha256,
        plugin.source_snapshot_sha256,
        capability_summary,
        permission_summary,
        blocked_summary,
        unsupported_summary,
        event_id
    ))
}

pub fn inspect_report(id: &str) -> Result<String, AppError> {
    let plugin = read_plugin(id)?;
    Ok(format!(
        "plugin inspect\n- id: {}\n- source runtime: {}\n- adapter version: {}\n- permission policy: {}\n- name: {}\n- version: {}\n- description: {}\n- status: {}\n- source path: {}\n- source manifest: {}\n- source manifest sha256: {}\n- source snapshot sha256: {}\n- imported path: {}\n- data path: {}\n- files: {}\n- directories: {}\n- capabilities: {}\n- required permissions: {}\n- blocked permissions: {}\n- unsupported: {}",
        plugin.id,
        plugin.source_label,
        plugin.adapter_version,
        plugin.permission_policy,
        plugin.name,
        plugin.version,
        plugin.description,
        plugin.status,
        plugin.source_path,
        plugin.manifest_path,
        plugin.source_manifest_sha256,
        plugin.source_snapshot_sha256,
        plugin_dir(&plugin.id).display(),
        plugin_data_path(&plugin.id).display(),
        plugin.files,
        plugin.directories,
        display_capabilities(&plugin.capabilities),
        display_vec(&plugin.required_permissions),
        display_vec(&plugin.blocked_permissions),
        display_vec(&plugin.unsupported)
    ))
}

pub fn validate_report(id: &str) -> Result<String, AppError> {
    let mut plugin = read_plugin(id)?;
    let manifest = verify_imported_snapshot(&mut plugin)?;

    plugin.status = "validated".to_string();
    write_plugin_manifest(&plugin)?;
    let event_id = state::record_event(
        "plugin.validated",
        "plugin static validation 완료",
        &format!("plugin_id={}", plugin.id),
    )?;

    Ok(format!(
        "plugin validate 결과\n- id: {}\n- status: {}\n- source manifest: {}\n- source manifest sha256: {}\n- source snapshot sha256: {}\n- capabilities: {}\n- required permissions: {}\n- blocked permissions: {}\n- unsupported: {}\n- ledger event: {}\n- 동작: manifest와 source snapshot hash가 normalized manifest와 일치함을 확인했습니다. 실행 권한은 아직 부여하지 않았습니다.",
        plugin.id,
        plugin.status,
        manifest.display(),
        plugin.source_manifest_sha256,
        plugin.source_snapshot_sha256,
        display_capabilities(&plugin.capabilities),
        display_vec(&plugin.required_permissions),
        display_vec(&plugin.blocked_permissions),
        display_vec(&plugin.unsupported),
        event_id
    ))
}

pub fn set_enabled_report(id: &str, enabled: bool) -> Result<String, AppError> {
    let mut plugin = read_plugin(id)?;
    if enabled {
        verify_imported_snapshot(&mut plugin)?;
    }
    plugin.status = if enabled { "enabled" } else { "disabled" }.to_string();
    write_plugin_manifest(&plugin)?;
    let event_type = if enabled {
        "plugin.enabled"
    } else {
        "plugin.disabled"
    };
    let summary = if enabled {
        "plugin enable 상태 기록"
    } else {
        "plugin disable 상태 기록"
    };
    let event_id = state::record_event(event_type, summary, &format!("plugin_id={}", plugin.id))?;

    Ok(format!(
        "plugin {} 결과\n- id: {}\n- status: {}\n- blocked permissions: {}\n- ledger event: {}\n- 동작: 상태만 변경했습니다. shell/MCP/background/file-write 권한은 여전히 기본 차단입니다.",
        if enabled { "enable" } else { "disable" },
        plugin.id,
        plugin.status,
        display_vec(&plugin.blocked_permissions),
        event_id
    ))
}

pub fn remove_report(id: &str, purge_data: bool) -> Result<String, AppError> {
    validate_plugin_id(id)?;
    let import_path = plugin_dir(id);
    if !import_path.exists() {
        return Err(AppError::usage(format!(
            "imported plugin을 찾지 못했습니다: {id}"
        )));
    }

    fs::remove_dir_all(&import_path).map_err(|err| {
        AppError::runtime(format!(
            "imported plugin package를 삭제하지 못했습니다: {} ({err})",
            import_path.display()
        ))
    })?;

    let data_path = plugin_data_path(id);
    let data_action = if purge_data && data_path.exists() {
        fs::remove_dir_all(&data_path).map_err(|err| {
            AppError::runtime(format!(
                "plugin data를 삭제하지 못했습니다: {} ({err})",
                data_path.display()
            ))
        })?;
        "plugin data 삭제"
    } else if purge_data {
        "plugin data 없음"
    } else {
        "plugin data 보존"
    };

    let event_id = state::record_event(
        "plugin.removed",
        "plugin package 제거",
        &format!("plugin_id={} purge_data={}", id, purge_data),
    )?;

    Ok(format!(
        "plugin remove 결과\n- id: {}\n- removed package: {}\n- data action: {}\n- ledger event: {}",
        id,
        import_path.display(),
        data_action,
        event_id
    ))
}

fn manifest_relative_path(source: PluginSource) -> &'static str {
    match source {
        PluginSource::Codex => ".codex-plugin/plugin.json",
        PluginSource::ClaudeCode => ".claude-plugin/plugin.json",
    }
}

fn fallback_directory_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

#[cfg(test)]
#[path = "plugin/tests.rs"]
mod tests;
