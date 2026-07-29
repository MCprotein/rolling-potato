use super::*;

pub(in crate::app::workflow_adapter::state) fn read_current_state_summary(
) -> Result<String, AppError> {
    let path = paths::current_state_file();
    if !path.exists() {
        return Ok("미초기화".to_string());
    }

    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;

    let identity = ledger::fresh_identity();
    match classify_current_state(&contents, &identity) {
        CurrentStateStatus::CleanNoActiveWorkflow => {
            Ok("초기화됨, active_workflow 없음".to_string())
        }
        CurrentStateStatus::CleanActiveWorkflow => {
            Ok("초기화됨, active_workflow 확인 필요".to_string())
        }
        CurrentStateStatus::Missing => Ok("미초기화".to_string()),
        CurrentStateStatus::Corrupt => Ok("손상됨, state reconcile 필요".to_string()),
        CurrentStateStatus::StaleProject => {
            Ok("stale project state, state reconcile 필요".to_string())
        }
    }
}

pub(in crate::app::workflow_adapter::state) fn current_state_status(
    identity: &RuntimeIdentity,
) -> Result<CurrentStateStatus, AppError> {
    let path = paths::current_state_file();
    if !path.exists() {
        return Ok(CurrentStateStatus::Missing);
    }

    let contents = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;

    Ok(classify_current_state(&contents, identity))
}

pub(in crate::app::workflow_adapter::state) fn classify_current_state(
    contents: &str,
    identity: &RuntimeIdentity,
) -> CurrentStateStatus {
    let Ok(snapshot) = parse_current_state(contents, "current-state classification") else {
        return CurrentStateStatus::Corrupt;
    };
    if snapshot.project_id != identity.project_id || snapshot.project_root != identity.project_root
    {
        return CurrentStateStatus::StaleProject;
    }
    match snapshot.active_workflow {
        None => CurrentStateStatus::CleanNoActiveWorkflow,
        Some(_) => CurrentStateStatus::CleanActiveWorkflow,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app::workflow_adapter::state) enum CurrentStateStatus {
    Missing,
    Corrupt,
    StaleProject,
    CleanNoActiveWorkflow,
    CleanActiveWorkflow,
}
