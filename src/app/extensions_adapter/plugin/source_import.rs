use super::*;

pub(super) struct SourcePlugin {
    pub(super) root: PathBuf,
    pub(super) manifest: PathBuf,
    manifest_text: String,
    manifest_sha256: String,
    snapshot_sha256: String,
    scan: DirectoryScan,
}

pub(super) fn inspect_source_plugin(
    source: PluginSource,
    raw_path: &str,
) -> Result<SourcePlugin, AppError> {
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

pub(super) fn normalize_plugin(
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
    let mut scan = source_plugin.scan.clone();
    apply_source_manifest_metadata(source, &source_plugin.manifest_text, &mut scan);
    let required_permissions = scan.required_permissions;
    let blocked_permissions = blocked_permissions(&required_permissions);
    let mut capabilities = scan.capabilities;
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
        unsupported: scan.unsupported,
    }
}

pub(super) fn apply_source_manifest_metadata(
    source: PluginSource,
    manifest_text: &str,
    scan: &mut DirectoryScan,
) {
    apply_manifest_risk_markers(manifest_text, &mut scan.required_permissions);
    if source == PluginSource::ClaudeCode {
        apply_claude_manifest_semantics(
            manifest_text,
            &mut scan.required_permissions,
            &mut scan.unsupported,
        );
    }
    finalize_permissions(&mut scan.required_permissions);
    finalize_unsupported(&mut scan.unsupported);
}
