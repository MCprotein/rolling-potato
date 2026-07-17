use super::*;

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
