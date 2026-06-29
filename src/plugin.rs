use crate::app::AppError;
use crate::cli::PluginSource;

use std::fs;
use std::path::{Path, PathBuf};

const ENTRY_LIMIT: usize = 10_000;

pub fn list_report() -> String {
    "plugin registry\n- 상태: persistent registry 미구현\n- 현재 가능: rpotato plugin import --from codex <local-path> --dry-run\n- marketplace, registry, catalog, remote URL import는 지원하지 않습니다."
        .to_string()
}

pub fn not_persisted_yet(action: &str, id: &str) -> Result<(), AppError> {
    Err(AppError::blocked(format!(
        "plugin {action}는 persistent registry 구현 후 활성화됩니다: {id}\n현재는 local plugin directory dry-run inspect만 가능합니다."
    )))
}

pub fn remove_not_persisted_yet(id: &str, purge_data: bool) -> Result<(), AppError> {
    let mode = if purge_data {
        "--purge-data"
    } else {
        "--keep-data"
    };

    Err(AppError::blocked(format!(
        "plugin remove는 persistent registry 구현 후 활성화됩니다: {id} {mode}\n현재는 삭제할 imported plugin registry가 없습니다."
    )))
}

pub fn import_report(
    source: PluginSource,
    raw_path: &str,
    dry_run: bool,
) -> Result<String, AppError> {
    reject_remote_or_marketplace(raw_path)?;

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

    let name = extract_json_string_field(&manifest_text, "name")
        .or_else(|| fallback_directory_name(&canonical_root))
        .unwrap_or_else(|| "unknown-plugin".to_string());
    let version = extract_json_string_field(&manifest_text, "version")
        .unwrap_or_else(|| "미기재".to_string());
    let description = extract_json_string_field(&manifest_text, "description")
        .unwrap_or_else(|| "미기재".to_string());

    let status = if dry_run {
        "dry-run: 파일 복사, registry 기록, capability enable을 수행하지 않았습니다."
    } else {
        return Err(AppError::blocked(format!(
            "plugin import 실행은 아직 차단되어 있습니다.\n먼저 dry-run inspect로 manifest와 권한 위험을 확인하세요: rpotato plugin import --from {} {} --dry-run",
            source.label(),
            raw_path
        )));
    };

    Ok(format!(
        "plugin import 검사\n- source runtime: {}\n- local path: {}\n- manifest: {}\n- name: {}\n- version: {}\n- description: {}\n- files: {}\n- directories: {}\n- remote source: 거부 정책 적용\n- marketplace/registry/catalog: 미지원\n- 실행 상태: {}",
        source.label(),
        canonical_root.display(),
        manifest.display(),
        name,
        version,
        description,
        scan.files,
        scan.directories,
        status
    ))
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

#[derive(Default)]
struct DirectoryScan {
    files: usize,
    directories: usize,
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

            if file_type.is_dir() {
                scan.directories += 1;
                stack.push(entry.path());
            } else if file_type.is_file() {
                scan.files += 1;
            }

            if scan.files + scan.directories > ENTRY_LIMIT {
                return Err(AppError::blocked(format!(
                    "plugin directory entry 수가 너무 많습니다. 현재 제한: {ENTRY_LIMIT}"
                )));
            }
        }
    }

    Ok(scan)
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

fn fallback_directory_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
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
    fn remote_plugin_import_is_blocked() {
        let err =
            import_report(PluginSource::Codex, "https://example.com/plugin.git", true).unwrap_err();

        assert_eq!(err.code, 3);
        assert!(err.message.contains("remote URL"));
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
