use super::skill;
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::integrity as checksum;
use crate::runtime_core::extensions::plugin::*;
use crate::surfaces::cli::command::PluginSource;

use std::fs;
use std::path::{Path, PathBuf};

mod registry;
mod scanner;

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

pub fn resolve_imported_codex_skill(
    id: &str,
) -> Result<Option<skill::ImportedSkillManifest>, AppError> {
    resolve_imported_codex_skill_inner(id, true)
}

fn resolve_imported_codex_skill_inner(
    id: &str,
    require_enabled: bool,
) -> Result<Option<skill::ImportedSkillManifest>, AppError> {
    let Some(tail) = id.strip_prefix("imported.codex.") else {
        return Ok(None);
    };
    let Some((plugin_name, skill_name)) = tail.split_once('.') else {
        return Ok(None);
    };
    validate_component_name(plugin_name, "plugin")?;
    validate_component_name(skill_name, "skill")?;
    let plugin_id = format!("imported.codex.{plugin_name}");
    let mut plugin = read_plugin(&plugin_id)?;
    if plugin.id != plugin_id {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- requested plugin: {}\n- stored plugin: {}\n- 이유: normalized manifest id binding이 다릅니다.",
            plugin_id, plugin.id
        )));
    }
    if plugin.source != PluginSource::Codex {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- skill: {id}\n- 이유: Codex plugin capability가 아닙니다."
        )));
    }
    if require_enabled && plugin.status != "enabled" {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- plugin: {}\n- skill: {}\n- status: {}\n- 다음: plugin validate와 plugin enable을 먼저 실행하세요.",
            plugin.id, id, plugin.status
        )));
    }
    if plugin.adapter_version != PLUGIN_ADAPTER_VERSION
        || plugin.permission_policy != PLUGIN_PERMISSION_POLICY
    {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- plugin: {}\n- 이유: adapter 또는 permission policy가 현재 실행 계약과 다릅니다.\n- stored adapter: {}\n- current adapter: {}\n- 다음: 신뢰하는 local directory에서 plugin을 다시 import하세요.",
            plugin.id, plugin.adapter_version, PLUGIN_ADAPTER_VERSION
        )));
    }
    let manifest_path = verify_imported_snapshot(&mut plugin)?;
    verify_execution_metadata(&mut plugin, &plugin_id, &manifest_path)?;

    let relative_path = format!("skills/{skill_name}/SKILL.md");
    let capability = plugin
        .capabilities
        .iter()
        .find(|capability| {
            capability.kind == "skill"
                && capability.path == relative_path
                && capability.status == "mapped"
                && capability.required_permission == "none"
        })
        .ok_or_else(|| {
            AppError::blocked(format!(
                "plugin skill 실행 차단\n- plugin: {}\n- skill: {}\n- 이유: canonical instruction-only SKILL.md capability가 아닙니다.\n- expected path: {}",
                plugin.id, id, relative_path
            ))
        })?;
    let skill_dir = plugin_dir(&plugin.id)
        .join("source")
        .join("skills")
        .join(skill_name);
    if plugin.capabilities.iter().any(|candidate| {
        candidate.required_permission != "none"
            && Path::new(&candidate.path).starts_with(Path::new(&format!("skills/{skill_name}")))
    }) {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- plugin: {}\n- skill: {}\n- 이유: skill directory에 별도 승인이 필요한 실행 capability가 포함되어 있습니다.",
            plugin.id, id
        )));
    }
    let path = skill_dir.join("SKILL.md");
    let metadata = fs::metadata(&path).map_err(|err| {
        AppError::usage(format!(
            "plugin SKILL.md metadata를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    if !metadata.is_file() || metadata.len() > IMPORTED_SKILL_MAX_BYTES {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- skill: {}\n- path: {}\n- 이유: SKILL.md는 {} bytes 이하 regular file이어야 합니다.",
            id,
            path.display(),
            IMPORTED_SKILL_MAX_BYTES
        )));
    }
    let text = fs::read_to_string(&path).map_err(|err| {
        AppError::usage(format!(
            "plugin SKILL.md를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let parsed = parse_codex_skill(&text, &path)?;
    if parsed.name != skill_name {
        return Err(AppError::blocked(format!(
            "plugin skill 실행 차단\n- skill: {}\n- path: {}\n- 이유: frontmatter name({})과 directory name({})이 다릅니다.",
            id,
            path.display(),
            parsed.name,
            skill_name
        )));
    }
    let source_sha256 = checksum::sha256_file(&path)?;
    Ok(Some(skill::ImportedSkillManifest {
        id: id.to_string(),
        display_name: parsed.name,
        description: parsed.description,
        instructions: parsed.instructions,
        plugin_id: plugin.id,
        source_path: capability.path.clone(),
        source_sha256,
    }))
}

pub fn revalidate_completed_codex_skill(
    id: &str,
    expected_source_path: &str,
    expected_source_sha256: &str,
) -> Result<skill::ImportedSkillManifest, AppError> {
    let manifest = resolve_imported_codex_skill_inner(id, false)?
        .ok_or_else(|| AppError::blocked("completed plugin skill manifest를 찾지 못했습니다."))?;
    if manifest.source_path != expected_source_path
        || manifest.source_sha256 != expected_source_sha256
    {
        return Err(AppError::blocked(format!(
            "completed plugin workflow source binding 불일치\n- skill: {}\n- expected path: {}\n- actual path: {}\n- expected sha256: {}\n- actual sha256: {}",
            id,
            expected_source_path,
            manifest.source_path,
            expected_source_sha256,
            manifest.source_sha256
        )));
    }
    Ok(manifest)
}

fn verify_execution_metadata(
    plugin: &mut PluginSnapshot,
    requested_id: &str,
    manifest_path: &Path,
) -> Result<(), AppError> {
    let source_dir = plugin_dir(requested_id).join("source");
    let manifest_text = fs::read_to_string(manifest_path).map_err(|err| {
        AppError::usage(format!(
            "plugin source manifest를 읽지 못했습니다: {} ({err})",
            manifest_path.display()
        ))
    })?;
    let source_name = extract_json_string_field(&manifest_text, "name").ok_or_else(|| {
        AppError::blocked(format!(
            "plugin execution metadata 차단\n- plugin: {requested_id}\n- 이유: source manifest name이 없습니다."
        ))
    })?;
    let expected_id = format!("imported.codex.{}", slug(&source_name));
    let mut scan = scan_directory(&source_dir, PluginSource::Codex)?;
    apply_manifest_risk_markers(&manifest_text, &mut scan.required_permissions);
    finalize_permissions(&mut scan.required_permissions);
    let expected_blocked = blocked_permissions(&scan.required_permissions);
    let mut actual_capabilities = capability_summary(&plugin.capabilities);
    let mut scanned_capabilities = capability_summary(&scan.capabilities);
    actual_capabilities.sort();
    scanned_capabilities.sort();
    let metadata_matches = plugin.id == requested_id
        && expected_id == requested_id
        && plugin.name == source_name
        && plugin.files == scan.files
        && plugin.directories == scan.directories
        && actual_capabilities == scanned_capabilities
        && plugin.required_permissions == scan.required_permissions
        && plugin.blocked_permissions == expected_blocked
        && plugin.unsupported == scan.unsupported;
    if metadata_matches {
        return Ok(());
    }

    plugin.status = "blocked".to_string();
    write_plugin_manifest(plugin)?;
    write_validation_report(plugin)?;
    let event_id = state::record_event(
        "plugin.validation.blocked",
        "plugin execution metadata drift 차단",
        &format!(
            "plugin_id={} reason=normalized-capability-metadata-mismatch",
            plugin.id
        ),
    )?;
    Err(AppError::blocked(format!(
        "plugin execution metadata 차단\n- plugin: {}\n- status: blocked\n- 이유: source 재스캔 결과와 normalized capability metadata가 다릅니다.\n- ledger event: {}\n- 다음: 신뢰하는 local directory에서 plugin을 다시 import하세요.",
        plugin.id, event_id
    )))
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
