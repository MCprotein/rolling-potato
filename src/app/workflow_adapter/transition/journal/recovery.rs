use super::*;

pub(crate) fn recover_pending_source_bundles() -> Result<usize, AppError> {
    if !recovery_work_may_exist() {
        return Ok(0);
    }
    let identity = if paths::current_state_file().exists() {
        crate::app::workflow_adapter::ledger::validated_current_identity()?
    } else {
        crate::app::workflow_adapter::ledger::fresh_identity()
    };
    let _guard = TransitionGuard::acquire(&identity.project_id)?;
    recover_pending_bundles_under_guard(&identity.project_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionLagReadStatus {
    Clear,
    Lagging,
    Unavailable,
}

pub(crate) fn projection_lag_status_read_only(project_id: &str) -> ProjectionLagReadStatus {
    let journal_directory = paths::project_transition_journal_dir(project_id);
    match validate_projection_lag_authority(project_id, &journal_directory) {
        Ok(false) => ProjectionLagReadStatus::Clear,
        Ok(true) => ProjectionLagReadStatus::Lagging,
        Err(_) => ProjectionLagReadStatus::Unavailable,
    }
}

pub(in crate::app::workflow_adapter::transition) fn recover_pending_bundles_under_guard(
    project_id: &str,
) -> Result<usize, AppError> {
    let directory = paths::project_transition_journal_dir(project_id);
    let lag_directory = paths::projection_lag_dir();
    if !directory.exists() && !lag_directory.exists() {
        return Ok(0);
    }
    validate_projection_lag_authority(project_id, &directory)?;
    if !directory.exists() {
        return Ok(0);
    }
    let mut entries = bounded_regular_entries(
        &directory,
        MAX_RECOVERY_JOURNAL_ENTRIES,
        MAX_RECOVERY_JOURNAL_BYTES,
        |_| true,
    )
    .map_err(|err| {
        AppError::blocked(format!(
            "transition journal recovery bound 검증 실패: {err}"
        ))
    })?;
    entries.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    let mut recovered = 0_usize;
    for entry in entries {
        let name = entry.name;
        if name == "transition.lock" {
            continue;
        }
        if let Some(intent_id) = name.strip_suffix(".prepared.json.tmp") {
            validate_ascii_id(intent_id, "intent")?;
            let final_path = paths::project_transition_journal_file(project_id, intent_id);
            let temp_body = read_regular_utf8_bounded(
                &entry.path,
                MAX_PREPARED_BUNDLE_BYTES,
                "transition temp",
            )?;
            let temp_bundle = parse_prepared_source_bundle(&temp_body)?;
            if temp_bundle.intent_id != intent_id || temp_bundle.project_id != project_id {
                return Err(AppError::blocked(
                    "transition temp owner/name binding 불일치",
                ));
            }
            if final_path.exists() {
                let final_body = read_regular_utf8_bounded(
                    &final_path,
                    MAX_PREPARED_BUNDLE_BYTES,
                    "transition final",
                )?;
                if final_body != temp_body {
                    return Err(AppError::blocked("transition final/temp bytes conflict"));
                }
            }
            fs::remove_file(&entry.path).map_err(|err| {
                AppError::runtime(format!("zero-effect transition temp cleanup 실패: {err}"))
            })?;
            sync_parent(&entry.path)?;
            continue;
        }
        let Some(intent_id) = name.strip_suffix(".prepared.json") else {
            return Err(AppError::blocked(format!(
                "unknown transition journal entry 보존: {name}"
            )));
        };
        validate_ascii_id(intent_id, "intent")?;
        let body =
            read_regular_utf8_bounded(&entry.path, MAX_PREPARED_BUNDLE_BYTES, "transition final")?;
        let bundle = parse_prepared_source_bundle(&body)?;
        if bundle.intent_id != intent_id || bundle.project_id != project_id {
            return Err(AppError::blocked(
                "transition final owner/name binding 불일치",
            ));
        }
        match bundle.intent_kind.as_str() {
            "approve-patch" if bundle.additional_members.is_empty() => {
                #[cfg(not(unix))]
                return Err(AppError::blocked(format!(
                    "source install recovery 차단\n- code: source-install.unsupported-platform\n- platform: {}\n- 동작: committed journal을 보존했습니다.",
                    std::env::consts::OS
                )));
                #[cfg(unix)]
                {
                    crate::app::workflow_adapter::state::validate_current_state_recovery_cas(
                        bundle.current_revision,
                        &bundle.current_artifact_hash,
                        None,
                    )?;
                    crate::app::workflow_adapter::state::install_prepared_source_bundle(
                        &bundle,
                        &entry.path,
                    )?;
                }
            }
            "approve-patch" => {
                #[cfg(not(unix))]
                return Err(AppError::blocked(format!(
                    "source install recovery 차단\n- code: source-install.unsupported-platform\n- platform: {}\n- 동작: committed journal을 보존했습니다.",
                    std::env::consts::OS
                )));
                #[cfg(unix)]
                crate::app::patch_adapter::recover_prepared_approval_bundle(&bundle, &entry.path)?;
            }
            "approve-verification" => {
                crate::app::patch_adapter::recover_prepared_verification_bundle(
                    &bundle,
                    &entry.path,
                )?;
            }
            kind if is_terminal_action_intent_kind(kind) => {
                crate::app::workflow_adapter::state::recover_project_current_state_prepared_terminal_action(
                    &bundle,
                    &entry.path,
                )?;
            }
            kind if is_state_transition_intent_kind(kind) => {
                crate::app::workflow_adapter::state::recover_prepared_state_transition(&bundle)?;
            }
            _ => return Err(AppError::blocked("transition recovery intent kind 불일치")),
        }
        remove_committed_source_bundle(&bundle, &entry.path)?;
        recovered = recovered
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("transition recovery count overflow"))?;
    }
    Ok(recovered)
}

fn validate_projection_lag_authority(
    project_id: &str,
    journal_directory: &Path,
) -> Result<bool, AppError> {
    let lag_directory = paths::projection_lag_dir();
    if !lag_directory.exists() {
        return Ok(false);
    }
    let lag_entries = bounded_regular_entries(
        &lag_directory,
        MAX_PROJECTION_LAG_ENTRIES,
        MAX_PROJECTION_LAG_BYTES,
        |name| name.ends_with(".json") || name.ends_with(".json.tmp"),
    )
    .map_err(|err| AppError::blocked(format!("projection lag recovery bound 검증 실패: {err}")))?;
    if lag_entries.is_empty() {
        return Ok(false);
    }
    let mut bundles = Vec::new();
    if journal_directory.exists() {
        let entries = bounded_regular_entries(
            journal_directory,
            MAX_RECOVERY_JOURNAL_ENTRIES,
            MAX_RECOVERY_JOURNAL_BYTES,
            |name| {
                name == "transition.lock"
                    || name.ends_with(".prepared.json")
                    || name.ends_with(".prepared.json.tmp")
            },
        )
        .map_err(|err| {
            AppError::blocked(format!(
                "projection lag journal recovery bound 검증 실패: {err}"
            ))
        })?;
        for entry in entries {
            let name = entry.name;
            if name == "transition.lock" || !name.ends_with(".prepared.json") {
                continue;
            }
            let body = read_regular_utf8_bounded(
                &entry.path,
                MAX_PREPARED_BUNDLE_BYTES,
                "projection lag journal",
            )?;
            let bundle = parse_prepared_source_bundle(&body)?;
            if bundle.project_id != project_id {
                return Err(AppError::blocked(
                    "projection lag journal project binding 불일치",
                ));
            }
            bundles.push(bundle);
        }
    }
    for entry in lag_entries {
        let name = entry.name;
        let final_name = name.strip_suffix(".tmp").unwrap_or(&name);
        if !final_name.ends_with(".json") {
            return Err(AppError::blocked(
                "unknown projection lag entry를 보존했습니다.",
            ));
        }
        let body =
            read_regular_utf8_bounded(&entry.path, MAX_PROJECTION_LAG_BYTES, "projection lag")?;
        let matches = bundles
            .iter()
            .filter(|bundle| {
                bundle.additional_members.iter().any(|member| {
                    member.kind == PreparedMemberKind::ProjectionLag
                        && member.bytes_utf8 == body
                        && Path::new(&member.path)
                            .file_name()
                            .and_then(|value| value.to_str())
                            == Some(final_name)
                })
            })
            .count();
        if matches != 1 {
            return Err(AppError::blocked(
                "orphan 또는 ambiguous projection lag를 보존했습니다.",
            ));
        }
    }
    Ok(true)
}
