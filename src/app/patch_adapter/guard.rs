use super::*;

pub(super) struct ApprovalLock {
    _lease: lease::RecoverableLease,
}

impl ApprovalLock {
    pub(super) fn acquire(proposal_id: &str) -> Result<Self, AppError> {
        let path = paths::project_patch_proposals_dir().join(format!("{proposal_id}.approve.lock"));
        lease::RecoverableLease::acquire(path, "patch approve").map(|lease| Self { _lease: lease })
    }
}

pub(super) fn approval_prelock_test_barrier() -> Result<(), AppError> {
    if !cfg!(debug_assertions) {
        return Ok(());
    }
    let Ok(base) = std::env::var("RPOTATO_TEST_APPROVAL_PRELOCK_BARRIER") else {
        return Ok(());
    };
    let ready = PathBuf::from(format!("{base}.ready"));
    let release = PathBuf::from(format!("{base}.release"));
    fs::write(&ready, b"ready")
        .map_err(|err| AppError::runtime(format!("approval test barrier 생성 실패: {err}")))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !release.exists() {
        if std::time::Instant::now() >= deadline {
            return Err(AppError::runtime("approval test barrier timeout"));
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(())
}

pub(crate) fn approval_transaction_fault(stage: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_APPROVAL_TRANSACTION_FAULT").as_deref() == Ok(stage)
    {
        return Err(AppError::runtime(format!(
            "injected prepared approval transaction fault: {stage}"
        )));
    }
    Ok(())
}

pub(crate) fn verification_approval_transaction_fault(stage: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_VERIFICATION_APPROVAL_FAULT").as_deref() == Ok(stage)
    {
        return Err(AppError::runtime(format!(
            "injected prepared verification approval fault: {stage}"
        )));
    }
    Ok(())
}

pub(crate) fn approval_projection_fault() -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_APPROVAL_PROJECTION_FAULT").as_deref() == Ok("converge")
    {
        return Err(AppError::runtime(
            "injected prepared approval projection convergence fault",
        ));
    }
    Ok(())
}

pub(super) fn load_workflow_under_approval_lock(
    workflow_id: &str,
) -> Result<(state::WorkflowRecord, Option<ApprovalLock>), AppError> {
    let discovered = state::load_workflow(workflow_id)?;
    if discovered.proposal_id.is_empty() {
        return Ok((discovered, None));
    }
    let lock = ApprovalLock::acquire(&discovered.proposal_id)?;
    let current = state::load_workflow(workflow_id)?;
    if current.proposal_id != discovered.proposal_id {
        return Err(AppError::blocked(
            "workflow 작업 차단\n- 이유: approval lease 획득 중 proposal binding이 변경되었습니다.",
        ));
    }
    Ok((current, Some(lock)))
}

pub(super) fn restore_bytes(
    target: &Path,
    contents: &[u8],
    expected_current_hash: &str,
    expected_hash: &str,
) -> RollbackResult {
    if let Ok(bytes) = fs::read(target) {
        if let RollbackAdmission::AlreadyRestored(result) = application_domain::admit_rollback(
            &sha256_bytes(&bytes),
            expected_hash,
            expected_current_hash,
        ) {
            return result;
        }
    }
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_ROLLBACK_FAULT").as_deref() == Ok("replace-failure")
    {
        return RollbackResult {
            restored: false,
            status: "restore-failed: injected rollback replace failure".to_string(),
        };
    }
    let current = match fs::read(target) {
        Ok(current) => match application_domain::admit_rollback(
            &sha256_bytes(&current),
            expected_hash,
            expected_current_hash,
        ) {
            RollbackAdmission::Ready => current,
            RollbackAdmission::AlreadyRestored(result) | RollbackAdmission::Conflict(result) => {
                return result;
            }
        },
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!("restore-failed: target read error: {err}"),
            }
        }
    };
    let plan = match transition::prepare_source_install_v1(
        &format!("intent-rollback-{}", &expected_hash[..16]),
        "proposal-rollback",
        target,
        &current,
        contents,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!(
                    "restore-failed: rollback plan preparation failed: {}",
                    err.message
                ),
            }
        }
    };
    let bundle = match transition::prepare_source_bundle(
        &format!("intent-rollback-{}", &expected_hash[..16]),
        None,
        plan,
        &current,
        contents,
    ) {
        Ok(bundle) => bundle,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!(
                    "restore-failed: rollback bundle preparation failed: {}",
                    err.message
                ),
            }
        }
    };
    let journal_path = match transition::commit_prepared_source_bundle(&bundle) {
        Ok(path) => path,
        Err(err) => {
            return RollbackResult {
                restored: false,
                status: format!(
                    "restore-failed: rollback journal commit failed: {}",
                    err.message
                ),
            }
        }
    };
    if let Err(err) = state::install_prepared_source_bundle(&bundle, &journal_path) {
        return RollbackResult {
            restored: false,
            status: format!("restore-failed: {}", err.message),
        };
    }
    if let Err(err) = transition::remove_committed_source_bundle(&bundle, &journal_path) {
        return RollbackResult {
            restored: false,
            status: format!(
                "restore-failed: rollback journal cleanup failed: {}",
                err.message
            ),
        };
    }
    match fs::read(target) {
        Ok(actual) => application_domain::restored_result(&sha256_bytes(&actual), expected_hash),
        Err(err) => RollbackResult {
            restored: false,
            status: format!("restore-failed: restored bytes reread error: {err}"),
        },
    }
}
