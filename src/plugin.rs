use crate::app::AppError;
use crate::cli::PluginSource;
use crate::{ledger, paths, state};

use std::fs;
use std::path::{Component, Path, PathBuf};

const ENTRY_LIMIT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginSnapshot {
    id: String,
    source: PluginSource,
    source_label: String,
    name: String,
    version: String,
    description: String,
    status: String,
    source_path: String,
    manifest_path: String,
    files: usize,
    directories: usize,
    required_permissions: Vec<String>,
    unsupported: Vec<String>,
}

#[derive(Default)]
struct DirectoryScan {
    files: usize,
    directories: usize,
    required_permissions: Vec<String>,
    unsupported: Vec<String>,
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
    let unsupported_summary = display_vec(&plugin.unsupported);

    if dry_run {
        return Ok(format!(
            "plugin import 검사\n- source runtime: {}\n- plugin id: {}\n- local path: {}\n- manifest: {}\n- name: {}\n- version: {}\n- description: {}\n- files: {}\n- directories: {}\n- required permissions: {}\n- unsupported: {}\n- remote source: 거부 정책 적용\n- marketplace/registry/catalog: 미지원\n- 실행 상태: dry-run: 파일 복사, registry 기록, capability enable을 수행하지 않았습니다.",
            source.label(),
            plugin.id,
            source_plugin.root.display(),
            source_plugin.manifest.display(),
            plugin.name,
            plugin.version,
            plugin.description,
            plugin.files,
            plugin.directories,
            permission_summary,
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
        "plugin import 결과\n- source runtime: {}\n- plugin id: {}\n- imported path: {}\n- data path: {}\n- status: {}\n- required permissions: {}\n- unsupported: {}\n- ledger event: {}\n- 동작: source snapshot과 normalized manifest를 저장했습니다. capability 실행 권한은 부여하지 않았습니다.",
        source.label(),
        plugin.id,
        plugin_dir(&plugin.id).display(),
        plugin_data_path(&plugin.id).display(),
        plugin.status,
        permission_summary,
        unsupported_summary,
        event_id
    ))
}

pub fn inspect_report(id: &str) -> Result<String, AppError> {
    let plugin = read_plugin(id)?;
    Ok(format!(
        "plugin inspect\n- id: {}\n- source runtime: {}\n- name: {}\n- version: {}\n- description: {}\n- status: {}\n- source path: {}\n- source manifest: {}\n- imported path: {}\n- data path: {}\n- files: {}\n- directories: {}\n- required permissions: {}\n- unsupported: {}",
        plugin.id,
        plugin.source_label,
        plugin.name,
        plugin.version,
        plugin.description,
        plugin.status,
        plugin.source_path,
        plugin.manifest_path,
        plugin_dir(&plugin.id).display(),
        plugin_data_path(&plugin.id).display(),
        plugin.files,
        plugin.directories,
        display_vec(&plugin.required_permissions),
        display_vec(&plugin.unsupported)
    ))
}

pub fn validate_report(id: &str) -> Result<String, AppError> {
    let mut plugin = read_plugin(id)?;
    let source_dir = plugin_dir(id).join("source");
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

    plugin.status = "validated".to_string();
    write_plugin_manifest(&plugin)?;
    let event_id = state::record_event(
        "plugin.validated",
        "plugin static validation 완료",
        &format!("plugin_id={}", plugin.id),
    )?;

    Ok(format!(
        "plugin validate 결과\n- id: {}\n- status: {}\n- source manifest: {}\n- required permissions: {}\n- unsupported: {}\n- ledger event: {}\n- 동작: manifest와 source snapshot 경계를 확인했습니다. 실행 권한은 아직 부여하지 않았습니다.",
        plugin.id,
        plugin.status,
        manifest.display(),
        display_vec(&plugin.required_permissions),
        display_vec(&plugin.unsupported),
        event_id
    ))
}

pub fn set_enabled_report(id: &str, enabled: bool) -> Result<String, AppError> {
    let mut plugin = read_plugin(id)?;
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
        "plugin {} 결과\n- id: {}\n- status: {}\n- ledger event: {}\n- 동작: 상태만 변경했습니다. shell/MCP/background/file-write 권한은 여전히 기본 차단입니다.",
        if enabled { "enable" } else { "disable" },
        plugin.id,
        plugin.status,
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

    let scan = scan_directory(&canonical_root)?;
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

    Ok(SourcePlugin {
        root: canonical_root,
        manifest,
        manifest_text,
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

    PluginSnapshot {
        id,
        source,
        source_label: source.label().to_string(),
        name,
        version,
        description,
        status: "imported".to_string(),
        source_path: raw_path.to_string(),
        manifest_path: source_plugin.manifest.display().to_string(),
        files: source_plugin.scan.files,
        directories: source_plugin.scan.directories,
        required_permissions: source_plugin.scan.required_permissions.clone(),
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
        name: required_field(&text, "displayName")?,
        version: required_field(&text, "version")?,
        description: required_field(&text, "description")?,
        status: required_field(&text, "status")?,
        source_path: required_field(&text, "sourcePath")?,
        manifest_path: required_field(&text, "sourceManifestPath")?,
        files: required_usize(&text, "files")?,
        directories: required_usize(&text, "directories")?,
        required_permissions: extract_json_string_array(&text, "requiredPermissions"),
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
        "{{\n  \"schemaVersion\": 1,\n  \"pluginId\": \"{}\",\n  \"requiredPermissions\": {},\n  \"unsupported\": {},\n  \"policy\": \"foreign plugin execution remains disabled until runtime policy approval\"\n}}\n",
        ledger::json_string(&plugin.id),
        json_array(&plugin.required_permissions),
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
            "{{\n  \"schemaVersion\": 1,\n  \"id\": \"{}\",\n  \"displayName\": \"{}\",\n  \"sourceRuntime\": \"{}\",\n  \"version\": \"{}\",\n  \"description\": \"{}\",\n  \"status\": \"{}\",\n  \"sourcePath\": \"{}\",\n  \"sourceManifestPath\": \"{}\",\n  \"files\": {},\n  \"directories\": {},\n  \"requiredPermissions\": {},\n  \"unsupported\": {}\n}}\n",
            ledger::json_string(&self.id),
            ledger::json_string(&self.name),
            ledger::json_string(&self.source_label),
            ledger::json_string(&self.version),
            ledger::json_string(&self.description),
            ledger::json_string(&self.status),
            ledger::json_string(&self.source_path),
            ledger::json_string(&self.manifest_path),
            self.files,
            self.directories,
            json_array(&self.required_permissions),
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

fn scan_directory(root: &Path) -> Result<DirectoryScan, AppError> {
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
            if file_type.is_dir() {
                if file_name == "bin" {
                    push_unique(&mut scan.required_permissions, "bin-executable");
                }
                if file_name == "mcp" || file_name == "mcp-servers" {
                    push_unique(&mut scan.required_permissions, "mcp-server");
                }
                if file_name == "hooks" {
                    push_unique(&mut scan.required_permissions, "hook");
                }
                scan.directories += 1;
                stack.push(entry.path());
            } else if file_type.is_file() {
                if file_name.ends_with(".sh")
                    || file_name.ends_with(".ps1")
                    || file_name.ends_with(".bat")
                    || file_name.ends_with(".cmd")
                {
                    push_unique(&mut scan.required_permissions, "shell-command");
                }
                if file_name.contains("background") || file_name.contains("monitor") {
                    push_unique(&mut scan.required_permissions, "background-process");
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

    if scan.required_permissions.is_empty() {
        scan.required_permissions.push("none".to_string());
    }
    if scan.unsupported.is_empty() {
        scan.unsupported.push("none".to_string());
    }

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

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(data_root).unwrap();
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
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
