use crate::app::AppError;
use crate::cli::PluginSource;
use crate::{checksum, ledger, paths, state};

use std::fs;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

const ENTRY_LIMIT: usize = 10_000;
const PLUGIN_MANIFEST_SCHEMA_VERSION: usize = 2;
const PLUGIN_ADAPTER_VERSION: &str = "rpotato-plugin-adapter-v0.27.0";
const PLUGIN_PERMISSION_POLICY: &str = "default-deny-external-capabilities-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginSnapshot {
    id: String,
    source: PluginSource,
    source_label: String,
    adapter_version: String,
    permission_policy: String,
    name: String,
    version: String,
    description: String,
    status: String,
    source_path: String,
    manifest_path: String,
    source_manifest_sha256: String,
    source_snapshot_sha256: String,
    files: usize,
    directories: usize,
    capabilities: Vec<PluginCapability>,
    required_permissions: Vec<String>,
    blocked_permissions: Vec<String>,
    unsupported: Vec<String>,
}

#[derive(Default)]
struct DirectoryScan {
    files: usize,
    directories: usize,
    capabilities: Vec<PluginCapability>,
    required_permissions: Vec<String>,
    unsupported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginCapability {
    kind: String,
    path: String,
    status: String,
    required_permission: String,
}

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

struct SourcePlugin {
    root: PathBuf,
    manifest: PathBuf,
    manifest_text: String,
    manifest_sha256: String,
    snapshot_sha256: String,
    scan: DirectoryScan,
}

fn inspect_source_plugin(source: PluginSource, raw_path: &str) -> Result<SourcePlugin, AppError> {
    reject_remote_or_marketplace(raw_path)?;
    reject_path_traversal(raw_path)?;

    let root = PathBuf::from(raw_path);
    if !root.exists() {
        return Err(AppError::usage(format!(
            "local plugin directory가 존재하지 않습니다: {raw_path}"
        )));
    }

    if !root.is_dir() {
        return Err(AppError::usage(format!(
            "plugin import 대상은 directory여야 합니다: {raw_path}"
        )));
    }

    let canonical_root = root.canonicalize().map_err(|err| {
        AppError::usage(format!(
            "plugin directory canonicalize 실패: {raw_path}\n이유: {err}"
        ))
    })?;

    let manifest = canonical_root.join(manifest_relative_path(source));
    if !manifest.exists() {
        return Err(AppError::usage(format!(
            "{} plugin manifest가 없습니다: {}\n필요한 파일: {}",
            source.label(),
            canonical_root.display(),
            manifest_relative_path(source)
        )));
    }

    if !manifest.is_file() {
        return Err(AppError::usage(format!(
            "plugin manifest는 file이어야 합니다: {}",
            manifest.display()
        )));
    }

    let scan = scan_directory(&canonical_root, source)?;
    let manifest_text = fs::read_to_string(&manifest).map_err(|err| {
        AppError::usage(format!(
            "plugin manifest를 읽을 수 없습니다: {}\n이유: {err}",
            manifest.display()
        ))
    })?;

    if !manifest_text.trim_start().starts_with('{') {
        return Err(AppError::usage(format!(
            "plugin manifest는 JSON object여야 합니다: {}",
            manifest.display()
        )));
    }
    let manifest_sha256 = checksum::sha256_file(&manifest)?;
    let snapshot_sha256 = sha256_directory_snapshot(&canonical_root)?;

    Ok(SourcePlugin {
        root: canonical_root,
        manifest,
        manifest_text,
        manifest_sha256,
        snapshot_sha256,
        scan,
    })
}

fn normalize_plugin(
    source: PluginSource,
    raw_path: &str,
    source_plugin: &SourcePlugin,
) -> PluginSnapshot {
    let name = extract_json_string_field(&source_plugin.manifest_text, "name")
        .or_else(|| fallback_directory_name(&source_plugin.root))
        .unwrap_or_else(|| "unknown-plugin".to_string());
    let version = extract_json_string_field(&source_plugin.manifest_text, "version")
        .unwrap_or_else(|| "미기재".to_string());
    let description = extract_json_string_field(&source_plugin.manifest_text, "description")
        .unwrap_or_else(|| "미기재".to_string());
    let id = format!("imported.{}.{}", source.label(), slug(&name));
    let mut required_permissions = source_plugin.scan.required_permissions.clone();
    apply_manifest_risk_markers(&source_plugin.manifest_text, &mut required_permissions);
    finalize_permissions(&mut required_permissions);
    let blocked_permissions = blocked_permissions(&required_permissions);
    let mut capabilities = source_plugin.scan.capabilities.clone();
    if capabilities.is_empty() {
        capabilities.push(PluginCapability::new(
            "manifest",
            manifest_relative_path(source),
            "mapped",
            "none",
        ));
    }

    PluginSnapshot {
        id,
        source,
        source_label: source.label().to_string(),
        adapter_version: PLUGIN_ADAPTER_VERSION.to_string(),
        permission_policy: PLUGIN_PERMISSION_POLICY.to_string(),
        name,
        version,
        description,
        status: "imported".to_string(),
        source_path: raw_path.to_string(),
        manifest_path: source_plugin.manifest.display().to_string(),
        source_manifest_sha256: source_plugin.manifest_sha256.clone(),
        source_snapshot_sha256: source_plugin.snapshot_sha256.clone(),
        files: source_plugin.scan.files,
        directories: source_plugin.scan.directories,
        capabilities,
        required_permissions,
        blocked_permissions,
        unsupported: source_plugin.scan.unsupported.clone(),
    }
}

fn persist_plugin(plugin: &PluginSnapshot, source_root: &Path) -> Result<(), AppError> {
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

fn verify_imported_snapshot(plugin: &mut PluginSnapshot) -> Result<PathBuf, AppError> {
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

fn read_plugins() -> Result<Vec<PluginSnapshot>, AppError> {
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

fn read_plugin(id: &str) -> Result<PluginSnapshot, AppError> {
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

fn write_plugin_manifest(plugin: &PluginSnapshot) -> Result<(), AppError> {
    fs::write(normalized_manifest_path(&plugin.id), plugin.to_json()).map_err(|err| {
        AppError::runtime(format!(
            "normalized plugin manifest를 기록하지 못했습니다: {} ({err})",
            normalized_manifest_path(&plugin.id).display()
        ))
    })
}

fn write_validation_report(plugin: &PluginSnapshot) -> Result<(), AppError> {
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

fn manifest_relative_path(source: PluginSource) -> &'static str {
    match source {
        PluginSource::Codex => ".codex-plugin/plugin.json",
        PluginSource::ClaudeCode => ".claude-plugin/plugin.json",
    }
}

fn reject_remote_or_marketplace(raw_path: &str) -> Result<(), AppError> {
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

fn reject_path_traversal(raw_path: &str) -> Result<(), AppError> {
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

fn scan_directory(root: &Path, source: PluginSource) -> Result<DirectoryScan, AppError> {
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
                if source == PluginSource::ClaudeCode && file_name == "commands" {
                    push_capability(&mut scan.capabilities, "command", &relative_path, "none");
                }
                if file_name == "skills" {
                    push_capability(&mut scan.capabilities, "skill", &relative_path, "none");
                }
                if file_name == "agents" {
                    push_capability(&mut scan.capabilities, "subagent", &relative_path, "none");
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
                classify_runtime_file(source, &relative_path, &mut scan);
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

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), AppError> {
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

fn validate_plugin_id(id: &str) -> Result<(), AppError> {
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

fn plugin_dir(id: &str) -> PathBuf {
    paths::imported_plugins_dir().join(id)
}

fn plugin_data_path(id: &str) -> PathBuf {
    paths::plugin_data_dir().join(id)
}

fn normalized_manifest_path(id: &str) -> PathBuf {
    plugin_dir(id).join("rpotato-plugin.json")
}

fn extract_json_string_field(text: &str, field: &str) -> Option<String> {
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

fn extract_json_string_array(text: &str, field: &str) -> Vec<String> {
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

fn required_field(text: &str, field: &str) -> Result<String, AppError> {
    extract_json_string_field(text, field).ok_or_else(|| {
        AppError::usage(format!(
            "normalized plugin manifest에 필수 field가 없습니다: {field}"
        ))
    })
}

fn required_usize(text: &str, field: &str) -> Result<usize, AppError> {
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

fn fallback_directory_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

fn slug(value: &str) -> String {
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

fn display_vec(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

impl PluginCapability {
    fn new(kind: &str, path: &str, status: &str, required_permission: &str) -> Self {
        Self {
            kind: kind.to_string(),
            path: path.to_string(),
            status: status.to_string(),
            required_permission: required_permission.to_string(),
        }
    }

    fn summary(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.kind, self.path, self.status, self.required_permission
        )
    }

    fn from_summary(value: &str) -> Option<Self> {
        let mut parts = value.splitn(4, '|');
        Some(Self {
            kind: parts.next()?.to_string(),
            path: parts.next()?.to_string(),
            status: parts.next()?.to_string(),
            required_permission: parts.next()?.to_string(),
        })
    }
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

fn push_capability(
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

fn classify_runtime_file(source: PluginSource, relative_path: &str, scan: &mut DirectoryScan) {
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
    }
    if lower.starts_with("apps/") || lower.starts_with("app-integrations/") {
        push_permission_and_capability(scan, "app-integration", relative_path, "remote-connector");
    }

    match source {
        PluginSource::Codex => {
            if lower.starts_with("skills/") && lower.ends_with("skill.md") {
                push_capability(&mut scan.capabilities, "skill", relative_path, "none");
            }
        }
        PluginSource::ClaudeCode => {
            if lower.starts_with("skills/") && lower.ends_with(".md") {
                push_capability(&mut scan.capabilities, "skill", relative_path, "none");
            }
            if lower.starts_with("commands/") {
                push_capability(&mut scan.capabilities, "command", relative_path, "none");
            }
            if lower.starts_with("agents/") {
                push_capability(&mut scan.capabilities, "subagent", relative_path, "none");
            }
            if lower.starts_with("lsp/") {
                push_permission_and_capability(scan, "lsp-server", relative_path, "lsp-server");
            }
            if lower.starts_with("monitors/") || lower.starts_with("monitor/") {
                push_permission_and_capability(
                    scan,
                    "monitor",
                    relative_path,
                    "background-process",
                );
            }
            if lower.starts_with("settings/") {
                push_permission_and_capability(
                    scan,
                    "runtime-settings",
                    relative_path,
                    "runtime-settings",
                );
            }
        }
    }
}

fn is_unsupported_plugin_asset(relative_path: &str) -> bool {
    let lower = relative_path.to_ascii_lowercase();
    lower.starts_with("marketplace/")
        || lower.contains("/marketplace/")
        || lower.starts_with("registry/")
        || lower.contains("/registry/")
        || lower.ends_with(".vsix")
}

fn apply_manifest_risk_markers(manifest_text: &str, required_permissions: &mut Vec<String>) {
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

fn finalize_permissions(required_permissions: &mut Vec<String>) {
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

fn finalize_unsupported(unsupported: &mut Vec<String>) {
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

fn blocked_permissions(required_permissions: &[String]) -> Vec<String> {
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

fn blocked_permissions_from_json(text: &str) -> Vec<String> {
    let blocked = extract_json_string_array(text, "blockedPermissions");
    if blocked.is_empty() {
        blocked_permissions(&extract_json_string_array(text, "requiredPermissions"))
    } else {
        blocked
    }
}

fn capability_summary(capabilities: &[PluginCapability]) -> Vec<String> {
    capabilities.iter().map(PluginCapability::summary).collect()
}

fn capability_summary_from_json(text: &str) -> Vec<PluginCapability> {
    extract_json_string_array(text, "capabilitySummary")
        .iter()
        .filter_map(|summary| PluginCapability::from_summary(summary))
        .collect()
}

fn display_capabilities(capabilities: &[PluginCapability]) -> String {
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

fn sha256_directory_snapshot(root: &Path) -> Result<String, AppError> {
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

#[cfg(test)]
mod tests {
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

        let err =
            import_report(PluginSource::ClaudeCode, root.to_str().unwrap(), true).unwrap_err();

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
}
