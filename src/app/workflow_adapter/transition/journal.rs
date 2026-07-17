use super::*;

pub(crate) struct TransitionGuard {
    project_id: String,
    _lease: lease::RecoverableLease,
}

impl TransitionGuard {
    pub(crate) fn acquire(project_id: &str) -> Result<Self, AppError> {
        validate_ascii_id(project_id, "project")?;
        fs::create_dir_all(paths::project_transition_journal_dir(project_id)).map_err(|err| {
            AppError::runtime(format!("transition journal directory 생성 실패: {err}"))
        })?;
        let lease = lease::RecoverableLease::acquire_with_wait(
            paths::project_transition_lock(project_id),
            "prepared transition journal",
            std::time::Duration::from_secs(5),
        )?;
        Ok(Self {
            project_id: project_id.to_string(),
            _lease: lease,
        })
    }

    pub(crate) fn acquire_for(
        project_id: &str,
        _intent: CurrentStateIntent,
    ) -> Result<Self, AppError> {
        let guard = Self::acquire(project_id)?;
        recover_pending_bundles_under_guard(project_id)?;
        Ok(guard)
    }

    pub(crate) fn commit(&self, bundle: &PreparedSourceBundle) -> Result<PathBuf, AppError> {
        if bundle.project_id != self.project_id {
            return Err(AppError::blocked(
                "transition guard/project bundle binding 불일치",
            ));
        }
        commit_prepared_source_bundle_under_guard(bundle)
    }

    pub(crate) fn remove(
        &self,
        bundle: &PreparedSourceBundle,
        path: &Path,
    ) -> Result<(), AppError> {
        if bundle.project_id != self.project_id {
            return Err(AppError::blocked(
                "transition cleanup guard/project binding 불일치",
            ));
        }
        remove_committed_source_bundle(bundle, path)
    }
}

pub(super) fn projection_lag_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_PROJECTION_LAG_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected projection lag fault: {point}"
        )));
    }
    Ok(())
}

pub(super) fn restore_removed_file(path: &Path, bytes: &[u8], label: &str) -> Result<(), AppError> {
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

mod codec;
mod recovery_io;
use recovery_io::bounded_regular_entries;
pub(super) use recovery_io::{read_regular_utf8_bounded, recovery_work_may_exist};

pub(crate) use codec::{parse_prepared_source_bundle, render_prepared_source_bundle};

pub(crate) fn commit_prepared_source_bundle(
    bundle: &PreparedSourceBundle,
) -> Result<PathBuf, AppError> {
    let guard = TransitionGuard::acquire_for(&bundle.project_id, CurrentStateIntent::ApprovePatch)?;
    guard.commit(bundle)
}

fn commit_prepared_source_bundle_under_guard(
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

pub(super) fn recover_pending_bundles_under_guard(project_id: &str) -> Result<usize, AppError> {
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
                crate::patch::recover_prepared_approval_bundle(&bundle, &entry.path)?;
            }
            "approve-verification" => {
                crate::patch::recover_prepared_verification_bundle(&bundle, &entry.path)?;
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
