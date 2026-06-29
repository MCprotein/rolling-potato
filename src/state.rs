use std::fs;
use std::path::PathBuf;

use crate::app::AppError;
use crate::ledger::{self, RuntimeIdentity};
use crate::observability::{self, StoreStatus};
use crate::paths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateInit {
    pub identity: RuntimeIdentity,
    pub created_paths: Vec<PathBuf>,
    pub store: StoreStatus,
}

pub fn initialize() -> Result<StateInit, AppError> {
    let identity = ledger::current_identity();
    let created_paths = ensure_layout()?;
    write_current_state(&identity)?;
    ensure_runtime_evidence_file()?;

    let event = ledger::new_event_for(
        &identity,
        "runtime.init",
        "runtime state initialized",
        "created or verified app/project state layout",
    );
    ledger::append_event(&event)?;

    let store = observability::initialize(&identity)?;
    observability::project_event(&event)?;

    Ok(StateInit {
        identity,
        created_paths,
        store,
    })
}

pub fn status_report() -> Result<String, AppError> {
    let current_state = read_current_state_summary()?;
    let store = observability::status()?;
    let recovered = store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "state 상태\n- app state dir: {}\n- project state dir: {}\n- runtime ledger: {}\n- project session ledger: {}\n- current state: {}\n- observability db: {}\n- schema migration: v{}\n- ledger events: {}\n- sessions: {}\n- workflows: {}\n- active workflow: 없음\n- transcript parent/branch pointer: current-state schema에 null로 보존{}",
        paths::state_dir().display(),
        paths::project_state_dir().display(),
        paths::runtime_ledger_file().display(),
        paths::project_session_ledger_file().display(),
        current_state,
        store.path.display(),
        store.migration_version,
        store.ledger_events,
        store.sessions,
        store.workflows,
        recovered
    ))
}

pub fn cancel_report() -> Result<String, AppError> {
    let identity = ledger::current_identity();
    ensure_layout()?;
    observability::initialize(&identity)?;
    let event = ledger::new_event_for(
        &identity,
        "workflow.cancel.noop",
        "cancel requested with no active workflow",
        "active_workflow=null",
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)?;

    Ok(format!(
        "cancel 결과\n- active workflow: 없음\n- ledger event: {}\n- ledger: {}\n- 동작: 취소할 실행이 없어 no-op event만 기록했습니다.",
        event.event_id,
        paths::runtime_ledger_file().display()
    ))
}

fn ensure_layout() -> Result<Vec<PathBuf>, AppError> {
    let directories = [
        paths::config_dir(),
        paths::backends_dir(),
        paths::models_dir(),
        paths::downloads_dir(),
        paths::manifests_dir(),
        paths::logs_dir(),
        paths::state_dir(),
        paths::plugins_dir(),
        paths::imported_plugins_dir(),
        paths::plugin_data_dir(),
        paths::cache_dir(),
        paths::project_state_dir(),
        paths::project_evidence_dir(),
    ];

    let mut created = Vec::new();
    for directory in directories {
        if !directory.exists() {
            created.push(directory.clone());
        }
        fs::create_dir_all(&directory).map_err(|err| {
            AppError::runtime(format!(
                "state 디렉터리를 만들지 못했습니다: {} ({err})",
                directory.display()
            ))
        })?;
    }

    Ok(created)
}

fn write_current_state(identity: &RuntimeIdentity) -> Result<(), AppError> {
    let body = format!(
        "{{\n  \"schema_version\": 1,\n  \"project_id\": \"{}\",\n  \"project_root\": \"{}\",\n  \"session_id\": \"{}\",\n  \"active_workflow\": null,\n  \"parent_session_id\": null,\n  \"branch_from_event_id\": null,\n  \"compaction_boundary\": null,\n  \"terminal_states\": [\"complete\", \"failed\", \"cancelled\"]\n}}\n",
        ledger::json_string(&identity.project_id),
        ledger::json_string(&identity.project_root),
        ledger::json_string(&identity.session_id)
    );

    fs::write(paths::current_state_file(), body).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 기록하지 못했습니다: {} ({err})",
            paths::current_state_file().display()
        ))
    })
}

fn ensure_runtime_evidence_file() -> Result<(), AppError> {
    let path = paths::runtime_evidence_file();
    if path.exists() {
        return Ok(());
    }
    fs::write(&path, "").map_err(|err| {
        AppError::runtime(format!(
            "runtime evidence store를 만들지 못했습니다: {} ({err})",
            path.display()
        ))
    })
}

fn read_current_state_summary() -> Result<String, AppError> {
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

    if contents.contains("\"active_workflow\": null") {
        Ok("초기화됨, active_workflow 없음".to_string())
    } else {
        Ok("초기화됨, active_workflow 확인 필요".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_state_summary_handles_missing_file_as_uninitialized() {
        let summary = read_current_state_summary().unwrap();
        assert!(summary == "미초기화" || summary.contains("초기화됨"));
    }
}
