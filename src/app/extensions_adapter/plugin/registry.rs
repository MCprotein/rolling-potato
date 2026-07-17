use super::scanner::{copy_dir_recursive, sha256_directory_snapshot};
use super::{manifest_relative_path, PLUGIN_MANIFEST_SCHEMA_VERSION};
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::extensions::plugin::{
    blocked_permissions_from_json, capability_summary, capability_summary_from_json,
    extract_json_string_array, extract_json_string_field, required_field, required_usize,
    validate_plugin_id, PluginCapability,
};
use crate::surfaces::cli::command::PluginSource;

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PluginSnapshot {
    pub(super) id: String,
    pub(super) source: PluginSource,
    pub(super) source_label: String,
    pub(super) adapter_version: String,
    pub(super) permission_policy: String,
    pub(super) name: String,
    pub(super) version: String,
    pub(super) description: String,
    pub(super) status: String,
    pub(super) source_path: String,
    pub(super) manifest_path: String,
    pub(super) source_manifest_sha256: String,
    pub(super) source_snapshot_sha256: String,
    pub(super) files: usize,
    pub(super) directories: usize,
    pub(super) capabilities: Vec<PluginCapability>,
    pub(super) required_permissions: Vec<String>,
    pub(super) blocked_permissions: Vec<String>,
    pub(super) unsupported: Vec<String>,
}

pub(super) fn persist_plugin(plugin: &PluginSnapshot, source_root: &Path) -> Result<(), AppError> {
    validate_plugin_id(&plugin.id)?;
    let import_path = plugin_dir(&plugin.id);
    if import_path.exists() {
        return Err(AppError::usage(format!(
            "이미 import된 plugin id입니다: {}\n기존 plugin을 제거한 뒤 다시 import하세요.",
            plugin.id
        )));
    }

    fs::create_dir_all(import_path.join("source")).map_err(|err| {
        AppError::runtime(format!(
            "plugin import directory를 만들지 못했습니다: {} ({err})",
            import_path.display()
        ))
    })?;
    fs::create_dir_all(plugin_data_path(&plugin.id)).map_err(|err| {
        AppError::runtime(format!(
            "plugin data directory를 만들지 못했습니다: {} ({err})",
            plugin_data_path(&plugin.id).display()
        ))
    })?;

    copy_dir_recursive(source_root, &import_path.join("source"))?;
    write_plugin_manifest(plugin)?;
    write_validation_report(plugin)?;
    Ok(())
}

pub(super) fn verify_imported_snapshot(plugin: &mut PluginSnapshot) -> Result<PathBuf, AppError> {
    let source_dir = plugin_dir(&plugin.id).join("source");
    let manifest = source_dir.join(manifest_relative_path(plugin.source));

    if !source_dir.is_dir() {
        return Err(AppError::usage(format!(
            "imported plugin source directory가 없습니다: {}",
            source_dir.display()
        )));
    }
    if !manifest.is_file() {
        return Err(AppError::usage(format!(
            "imported plugin source manifest가 없습니다: {}",
            manifest.display()
        )));
    }

    let actual_manifest_sha256 = checksum::sha256_file(&manifest)?;
    let actual_snapshot_sha256 = sha256_directory_snapshot(&source_dir)?;
    let mut blockers = Vec::new();

    if plugin.source_manifest_sha256.is_empty() {
        blockers.push("stored source manifest sha256 missing; re-import required".to_string());
    } else if plugin.source_manifest_sha256 != actual_manifest_sha256 {
        blockers.push(format!(
            "source manifest hash mismatch: expected {} actual {}",
            plugin.source_manifest_sha256, actual_manifest_sha256
        ));
    }

    if plugin.source_snapshot_sha256.is_empty() {
        blockers.push("stored source snapshot sha256 missing; re-import required".to_string());
    } else if plugin.source_snapshot_sha256 != actual_snapshot_sha256 {
        blockers.push(format!(
            "source snapshot hash mismatch: expected {} actual {}",
            plugin.source_snapshot_sha256, actual_snapshot_sha256
        ));
    }

    if !blockers.is_empty() {
        plugin.status = "blocked".to_string();
        write_plugin_manifest(plugin)?;
        write_validation_report(plugin)?;
        let event_id = state::record_event(
            "plugin.validation.blocked",
            "plugin source snapshot drift 차단",
            &format!("plugin_id={} blockers={}", plugin.id, blockers.join("; ")),
        )?;
        return Err(AppError::blocked(format!(
            "plugin validation blocked\n- id: {}\n- status: blocked\n- blockers:\n- {}\n- ledger event: {}\n- 다음 단계: plugin source를 신뢰할 수 있는 local directory에서 다시 import하세요.",
            plugin.id,
            blockers.join("\n- "),
            event_id
        )));
    }

    Ok(manifest)
}

pub(super) fn read_plugins() -> Result<Vec<PluginSnapshot>, AppError> {
    let dir = paths::imported_plugins_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut plugins = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|err| {
        AppError::runtime(format!(
            "plugin registry directory를 읽지 못했습니다: {} ({err})",
            dir.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AppError::runtime(format!(
                "plugin registry entry를 읽지 못했습니다: {} ({err})",
                dir.display()
            ))
        })?;
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            let id = entry.file_name().to_string_lossy().to_string();
            if let Ok(plugin) = read_plugin(&id) {
                plugins.push(plugin);
            }
        }
    }

    plugins.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(plugins)
}

pub(super) fn read_plugin(id: &str) -> Result<PluginSnapshot, AppError> {
    validate_plugin_id(id)?;
    let manifest = normalized_manifest_path(id);
    let text = fs::read_to_string(&manifest).map_err(|err| {
        AppError::usage(format!(
            "plugin manifest를 읽지 못했습니다: {} ({err})",
            manifest.display()
        ))
    })?;

    let source_label = required_field(&text, "sourceRuntime")?;
    let source = PluginSource::parse(&source_label).ok_or_else(|| {
        AppError::usage(format!(
            "알 수 없는 plugin source runtime입니다: {source_label}"
        ))
    })?;

    Ok(PluginSnapshot {
        id: required_field(&text, "id")?,
        source,
        source_label,
        adapter_version: extract_json_string_field(&text, "adapterVersion")
            .unwrap_or_else(|| "legacy".to_string()),
        permission_policy: extract_json_string_field(&text, "permissionPolicy")
            .unwrap_or_else(|| "legacy-default-deny".to_string()),
        name: required_field(&text, "displayName")?,
        version: required_field(&text, "version")?,
        description: required_field(&text, "description")?,
        status: required_field(&text, "status")?,
        source_path: required_field(&text, "sourcePath")?,
        manifest_path: required_field(&text, "sourceManifestPath")?,
        source_manifest_sha256: extract_json_string_field(&text, "sourceManifestSha256")
            .unwrap_or_default(),
        source_snapshot_sha256: extract_json_string_field(&text, "sourceSnapshotSha256")
            .unwrap_or_default(),
        files: required_usize(&text, "files")?,
        directories: required_usize(&text, "directories")?,
        capabilities: capability_summary_from_json(&text),
        required_permissions: extract_json_string_array(&text, "requiredPermissions"),
        blocked_permissions: blocked_permissions_from_json(&text),
        unsupported: extract_json_string_array(&text, "unsupported"),
    })
}

pub(super) fn write_plugin_manifest(plugin: &PluginSnapshot) -> Result<(), AppError> {
    fs::write(normalized_manifest_path(&plugin.id), plugin.to_json()).map_err(|err| {
        AppError::runtime(format!(
            "normalized plugin manifest를 기록하지 못했습니다: {} ({err})",
            normalized_manifest_path(&plugin.id).display()
        ))
    })
}

pub(super) fn write_validation_report(plugin: &PluginSnapshot) -> Result<(), AppError> {
    let body = format!(
        "{{\n  \"schemaVersion\": {},\n  \"pluginId\": \"{}\",\n  \"adapterVersion\": \"{}\",\n  \"permissionPolicy\": \"{}\",\n  \"sourceManifestSha256\": \"{}\",\n  \"sourceSnapshotSha256\": \"{}\",\n  \"capabilities\": {},\n  \"capabilitySummary\": {},\n  \"requiredPermissions\": {},\n  \"blockedPermissions\": {},\n  \"unsupported\": {},\n  \"policy\": \"foreign plugin execution remains disabled until runtime policy approval\"\n}}\n",
        PLUGIN_MANIFEST_SCHEMA_VERSION,
        ledger::json_string(&plugin.id),
        ledger::json_string(&plugin.adapter_version),
        ledger::json_string(&plugin.permission_policy),
        ledger::json_string(&plugin.source_manifest_sha256),
        ledger::json_string(&plugin.source_snapshot_sha256),
        json_capabilities(&plugin.capabilities),
        json_array(&capability_summary(&plugin.capabilities)),
        json_array(&plugin.required_permissions),
        json_array(&plugin.blocked_permissions),
        json_array(&plugin.unsupported)
    );
    fs::write(plugin_dir(&plugin.id).join("validation-report.json"), body).map_err(|err| {
        AppError::runtime(format!(
            "plugin validation report를 기록하지 못했습니다: {} ({err})",
            plugin_dir(&plugin.id)
                .join("validation-report.json")
                .display()
        ))
    })
}

pub(super) fn plugin_dir(id: &str) -> PathBuf {
    paths::imported_plugins_dir().join(id)
}

pub(super) fn plugin_data_path(id: &str) -> PathBuf {
    paths::plugin_data_dir().join(id)
}

pub(super) fn normalized_manifest_path(id: &str) -> PathBuf {
    plugin_dir(id).join("rpotato-plugin.json")
}

impl PluginSnapshot {
    fn to_json(&self) -> String {
        format!(
            "{{\n  \"schemaVersion\": {},\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"sourceRuntime\": \"{}\",\n  \"adapterVersion\": \"{}\",\n  \"permissionPolicy\": \"{}\",\n  \"version\": \"{}\",\n  \"description\": \"{}\",\n  \"status\": \"{}\",\n  \"sourcePath\": \"{}\",\n  \"sourceManifestPath\": \"{}\",\n  \"sourceManifestSha256\": \"{}\",\n  \"sourceSnapshotSha256\": \"{}\",\n  \"files\": {},\n  \"directories\": {},\n  \"capabilities\": {},\n  \"capabilitySummary\": {},\n  \"requiredPermissions\": {},\n  \"blockedPermissions\": {},\n  \"unsupported\": {}\n}}\n",
            PLUGIN_MANIFEST_SCHEMA_VERSION,
            ledger::json_string(&self.id),
            ledger::json_string(&self.name),
            ledger::json_string(&self.source_label),
            ledger::json_string(&self.adapter_version),
            ledger::json_string(&self.permission_policy),
            ledger::json_string(&self.version),
            ledger::json_string(&self.description),
            ledger::json_string(&self.status),
            ledger::json_string(&self.source_path),
            ledger::json_string(&self.manifest_path),
            ledger::json_string(&self.source_manifest_sha256),
            ledger::json_string(&self.source_snapshot_sha256),
            self.files,
            self.directories,
            json_capabilities(&self.capabilities),
            json_array(&capability_summary(&self.capabilities)),
            json_array(&self.required_permissions),
            json_array(&self.blocked_permissions),
            json_array(&self.unsupported)
        )
    }
}

fn json_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", ledger::json_string(value)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn json_capabilities(capabilities: &[PluginCapability]) -> String {
    format!(
        "[{}]",
        capabilities
            .iter()
            .map(|capability| {
                format!(
                    "{{\"kind\":\"{}\",\"path\":\"{}\",\"status\":\"{}\",\"requiredPermission\":\"{}\"}}",
                    ledger::json_string(&capability.kind),
                    ledger::json_string(&capability.path),
                    ledger::json_string(&capability.status),
                    ledger::json_string(&capability.required_permission)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}
