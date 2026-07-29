use super::*;

pub(in crate::app::workflow_adapter::transition) fn projection_lag_fault(
    point: &str,
) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_PROJECTION_LAG_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected projection lag fault: {point}"
        )));
    }
    Ok(())
}

pub(in crate::app::workflow_adapter::transition) fn restore_removed_file(
    path: &Path,
    bytes: &[u8],
    label: &str,
) -> Result<(), AppError> {
    if path.exists() {
        if fs::read(path)
            .map_err(|err| AppError::runtime(format!("{label} restore reread 실패: {err}")))?
            != bytes
        {
            return Err(AppError::blocked(format!(
                "{label} restore immutable conflict"
            )));
        }
        return Ok(());
    }
    let temporary = path.with_extension("restore.tmp");
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|err| {
            AppError::runtime(format!("{label} restore temp cleanup 실패: {err}"))
        })?;
    }
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
        .map_err(|err| AppError::runtime(format!("{label} restore temp 생성 실패: {err}")))?;
    file.write_all(bytes)
        .map_err(|err| AppError::runtime(format!("{label} restore write 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("{label} restore fsync 실패: {err}")))?;
    fs::rename(&temporary, path)
        .map_err(|err| AppError::runtime(format!("{label} restore rename 실패: {err}")))?;
    sync_parent(path)
}

pub(crate) fn commit_prepared_source_bundle(
    bundle: &PreparedSourceBundle,
) -> Result<PathBuf, AppError> {
    let guard = TransitionGuard::acquire_for(&bundle.project_id, CurrentStateIntent::ApprovePatch)?;
    guard.commit(bundle)
}

pub(super) fn commit_prepared_source_bundle_under_guard(
    bundle: &PreparedSourceBundle,
) -> Result<PathBuf, AppError> {
    let body = render_prepared_source_bundle(bundle)?;
    let final_path = paths::project_transition_journal_file(&bundle.project_id, &bundle.intent_id);
    let temp_path = paths::project_transition_journal_temp(&bundle.project_id, &bundle.intent_id);
    validate_no_competing_prepared_journal(bundle, &final_path, &temp_path)?;
    if final_path.exists() {
        let existing = fs::read_to_string(&final_path)
            .map_err(|err| AppError::blocked(format!("prepared journal 읽기 실패: {err}")))?;
        let parsed = parse_prepared_source_bundle(&existing)?;
        if parsed != *bundle || existing != body {
            return Err(AppError::blocked("prepared journal immutable conflict"));
        }
        if temp_path.exists() {
            let temp = fs::read_to_string(&temp_path)
                .map_err(|err| AppError::blocked(format!("prepared temp 읽기 실패: {err}")))?;
            if temp != existing {
                return Err(AppError::blocked("prepared journal/temp conflict"));
            }
            fs::remove_file(&temp_path)
                .map_err(|err| AppError::runtime(format!("prepared temp cleanup 실패: {err}")))?;
            sync_parent(&temp_path)?;
        }
        return Ok(final_path);
    }
    if temp_path.exists() {
        let temp = fs::read_to_string(&temp_path)
            .map_err(|err| AppError::blocked(format!("prepared temp 읽기 실패: {err}")))?;
        if temp != body {
            return Err(AppError::blocked("prepared temp immutable conflict"));
        }
        parse_prepared_source_bundle(&temp)?;
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
            .open(&temp_path)
            .map_err(|err| AppError::runtime(format!("prepared temp create-new 실패: {err}")))?;
        file.write_all(body.as_bytes())
            .map_err(|err| AppError::runtime(format!("prepared temp write 실패: {err}")))?;
        file.sync_all()
            .map_err(|err| AppError::runtime(format!("prepared temp fsync 실패: {err}")))?;
    }
    fs::rename(&temp_path, &final_path)
        .map_err(|err| AppError::runtime(format!("prepared journal rename 실패: {err}")))?;
    sync_parent(&final_path)?;
    let installed = fs::read_to_string(&final_path)
        .map_err(|err| AppError::blocked(format!("prepared journal reread 실패: {err}")))?;
    if installed != body || parse_prepared_source_bundle(&installed)? != *bundle {
        return Err(AppError::blocked("prepared journal installed bytes 불일치"));
    }
    Ok(final_path)
}

fn validate_no_competing_prepared_journal(
    bundle: &PreparedSourceBundle,
    final_path: &Path,
    temp_path: &Path,
) -> Result<(), AppError> {
    let directory = paths::project_transition_journal_dir(&bundle.project_id);
    for entry in fs::read_dir(&directory)
        .map_err(|err| AppError::blocked(format!("transition journal discovery 실패: {err}")))?
    {
        let entry = entry
            .map_err(|err| AppError::blocked(format!("transition journal entry 실패: {err}")))?;
        let path = entry.path();
        if path == final_path || path == temp_path {
            continue;
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| AppError::blocked("transition journal filename UTF-8 불일치"))?
            .to_string();
        if name == "transition.lock" {
            continue;
        }
        if name.ends_with(".prepared.json") || name.ends_with(".prepared.json.tmp") {
            return Err(AppError::blocked(format!(
                "competing prepared journal 차단\n- pending: {name}\n- requested intent: {}\n- 동작: 새 journal을 만들지 않았습니다.",
                bundle.intent_id
            )));
        }
        return Err(AppError::blocked(format!(
            "unknown transition journal entry 보존: {name}"
        )));
    }
    Ok(())
}

pub(crate) fn remove_committed_source_bundle(
    bundle: &PreparedSourceBundle,
    path: &Path,
) -> Result<(), AppError> {
    let expected = paths::project_transition_journal_file(&bundle.project_id, &bundle.intent_id);
    if path != expected {
        return Err(AppError::blocked(
            "prepared journal cleanup path binding 불일치",
        ));
    }
    let body = fs::read_to_string(path)
        .map_err(|err| AppError::blocked(format!("prepared journal cleanup read 실패: {err}")))?;
    if parse_prepared_source_bundle(&body)? != *bundle {
        return Err(AppError::blocked("prepared journal cleanup binding 불일치"));
    }
    fs::remove_file(path)
        .map_err(|err| AppError::runtime(format!("prepared journal cleanup 실패: {err}")))?;
    let cleanup = projection_lag_fault("journal-remove")
        .and_then(|_| projection_lag_fault("journal-parent-fsync"))
        .and_then(|_| sync_parent(path));
    if let Err(error) = cleanup {
        restore_removed_file(path, body.as_bytes(), "prepared journal")?;
        return Err(error);
    }
    Ok(())
}

pub(crate) fn validate_committed_bundle_cleanup_authority(
    bundle: &PreparedSourceBundle,
    journal: &Path,
) -> Result<(), AppError> {
    validate_prepared_source_bundle(bundle)?;
    let expected = paths::project_transition_journal_file(&bundle.project_id, &bundle.intent_id);
    if journal != expected {
        return Err(AppError::blocked(
            "prepared cleanup journal path binding 불일치",
        ));
    }
    let body = fs::read_to_string(journal)
        .map_err(|err| AppError::blocked(format!("prepared cleanup journal 읽기 실패: {err}")))?;
    if parse_prepared_source_bundle(&body)? != *bundle {
        return Err(AppError::blocked(
            "prepared cleanup journal bytes binding 불일치",
        ));
    }
    if let Some(member) = bundle
        .additional_members
        .iter()
        .find(|member| member.kind == PreparedMemberKind::ProjectionLag)
    {
        let name = Path::new(&member.path)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::blocked("prepared cleanup lag filename 불일치"))?;
        let path = paths::projection_lag_dir().join(name);
        let temporary = path.with_extension("json.tmp");
        if temporary.exists() {
            return Err(AppError::blocked(
                "prepared cleanup lag temp가 남아 있어 증거를 보존했습니다.",
            ));
        }
        if path.exists()
            && fs::read(&path).map_err(|err| {
                AppError::blocked(format!("prepared cleanup lag 읽기 실패: {err}"))
            })? != member.bytes_utf8.as_bytes()
        {
            return Err(AppError::blocked(
                "prepared cleanup lag/member binding 불일치",
            ));
        }
    }
    Ok(())
}
