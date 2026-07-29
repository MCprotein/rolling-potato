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
