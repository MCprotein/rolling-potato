use super::super::*;

pub(crate) fn prepare_projection_lag_member(
    intent_id: &str,
    planned: &[crate::app::workflow_adapter::ledger::PlannedEvent],
) -> Result<PreparedMember, AppError> {
    validate_ascii_id(intent_id, "intent")?;
    if planned.len() != 10 {
        return Err(AppError::blocked(
            "projection lag는 exact E0..E9 plan이 필요합니다.",
        ));
    }
    let final_event = &planned[9];
    let required_event_ids = planned
        .iter()
        .map(|entry| {
            format!(
                "\"{}\"",
                crate::app::workflow_adapter::ledger::json_string(&entry.event.event_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let bytes_utf8 = format!(
        "{{\"schema_version\":1,\"intent_id\":\"{}\",\"event_id\":\"{}\",\"event_ordinal\":{},\"event_hash\":\"{}\",\"required_outputs\":[\"project-session-ledger\",\"global-operation-log\",\"sqlite\"],\"required_event_ids\":[{}]}}",
        crate::app::workflow_adapter::ledger::json_string(intent_id),
        crate::app::workflow_adapter::ledger::json_string(&final_event.event.event_id),
        final_event.ordinal,
        final_event.event_hash,
        required_event_ids,
    );
    let hash = sha256_bytes(bytes_utf8.as_bytes());
    Ok(PreparedMember {
        kind: PreparedMemberKind::ProjectionLag,
        path: format!(
            "state/projection-lag/{}-{}.json",
            intent_id, final_event.event.event_id
        ),
        schema_version: 1,
        binding: PreparedMemberBinding {
            artifact_id: Some(format!("projection-lag-{hash}")),
            causal_id: None,
            source_key: None,
            event_id: Some(final_event.event.event_id.clone()),
        },
        bytes_utf8,
        expected_type: "absent".to_string(),
        expected_identity: None,
        readonly: false,
        mode: 0o600,
        ownership: None,
        semantic_role_rank: 0,
    })
}

pub(crate) fn install_projection_lag(bundle: &PreparedSourceBundle) -> Result<PathBuf, AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let event_id = member
        .binding
        .event_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared projection lag event binding 누락"))?;
    let path = paths::projection_lag_file(&bundle.intent_id, event_id);
    let expected_stored = format!(
        "state/projection-lag/{}-{}.json",
        bundle.intent_id, event_id
    );
    if member.path != expected_stored {
        return Err(AppError::blocked(
            "prepared projection lag path binding 불일치",
        ));
    }
    if path.exists() {
        let existing = fs::read_to_string(&path)
            .map_err(|err| AppError::blocked(format!("projection lag reread 실패: {err}")))?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag immutable conflict"));
        }
        return Ok(path);
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::blocked("projection lag parent 누락"))?;
    fs::create_dir_all(parent)
        .map_err(|err| AppError::runtime(format!("projection lag directory 생성 실패: {err}")))?;
    let temporary = path.with_extension("json.tmp");
    if temporary.exists() {
        let existing = fs::read_to_string(&temporary)
            .map_err(|err| AppError::blocked(format!("projection lag temp reread 실패: {err}")))?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag temp immutable conflict"));
        }
    } else {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options
            .open(&temporary)
            .map_err(|err| AppError::runtime(format!("projection lag temp 생성 실패: {err}")))?;
        projection_lag_fault("temp-create")?;
        file.write_all(member.bytes_utf8.as_bytes())
            .map_err(|err| AppError::runtime(format!("projection lag temp write 실패: {err}")))?;
        projection_lag_fault("temp-write")?;
        file.sync_all()
            .map_err(|err| AppError::runtime(format!("projection lag temp fsync 실패: {err}")))?;
        projection_lag_fault("temp-fsync")?;
    }
    fs::rename(&temporary, &path)
        .map_err(|err| AppError::runtime(format!("projection lag rename 실패: {err}")))?;
    projection_lag_fault("rename")?;
    projection_lag_fault("parent-fsync")?;
    sync_parent(&path)?;
    Ok(path)
}

pub(crate) fn projection_lag_path(bundle: &PreparedSourceBundle) -> Result<PathBuf, AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let event_id = member
        .binding
        .event_id
        .as_deref()
        .ok_or_else(|| AppError::blocked("prepared projection lag event binding 누락"))?;
    Ok(paths::projection_lag_file(&bundle.intent_id, event_id))
}

pub(crate) fn remove_projection_lag(bundle: &PreparedSourceBundle) -> Result<(), AppError> {
    validate_prepared_source_bundle(bundle)?;
    let member = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .ok_or_else(|| AppError::blocked("prepared projection lag member 누락"))?;
    let path = projection_lag_path(bundle)?;
    let temporary = path.with_extension("json.tmp");
    if temporary.exists() {
        let existing = fs::read_to_string(&temporary).map_err(|err| {
            AppError::blocked(format!("projection lag temp cleanup read 실패: {err}"))
        })?;
        if existing != member.bytes_utf8 {
            return Err(AppError::blocked("projection lag temp cleanup conflict"));
        }
        fs::remove_file(&temporary)
            .map_err(|err| AppError::runtime(format!("projection lag temp cleanup 실패: {err}")))?;
        sync_parent(&temporary)?;
    }
    if !path.exists() {
        return Ok(());
    }
    let existing = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("projection lag cleanup read 실패: {err}")))?;
    if existing != member.bytes_utf8 {
        return Err(AppError::blocked(
            "projection lag cleanup immutable conflict",
        ));
    }
    fs::remove_file(&path)
        .map_err(|err| AppError::runtime(format!("projection lag cleanup 실패: {err}")))?;
    let cleanup = projection_lag_fault("lag-remove")
        .and_then(|_| projection_lag_fault("lag-parent-fsync"))
        .and_then(|_| sync_parent(&path));
    if let Err(error) = cleanup {
        restore_removed_file(&path, member.bytes_utf8.as_bytes(), "projection lag")?;
        return Err(error);
    }
    Ok(())
}
