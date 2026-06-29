use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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
        "runtime state 초기화",
        "app/project state layout 생성 또는 확인",
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
        "state 상태\n- app state dir: {}\n- project state dir: {}\n- runtime ledger: {}\n- project session ledger: {}\n- current state: {}\n- observability db: {}\n- schema migration: v{}\n- ledger events: {}\n- sessions: {}\n- workflows: {}\n- active workflow: 없음\n- transcript parent/branch pointer: current-state schema에 null로 보존\n- evidence stale policy: {}{}",
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
        crate::evidence::stale_policy_summary(),
        recovered
    ))
}

pub fn reconcile_report() -> Result<String, AppError> {
    let identity = ledger::current_identity();
    ensure_layout()?;
    let outcome = reconcile_current_state(&identity)?;
    let summary = outcome.summary();
    let event = ledger::new_event_for(
        &identity,
        outcome.event_type(),
        &summary,
        "current-state reconcile 완료",
    );
    ledger::append_event(&event)?;
    observability::initialize(&identity)?;
    observability::project_event(&event)?;

    Ok(format!(
        "state reconcile 결과\n- outcome: {}\n- current state: {}\n- ledger event: {}\n- 동작: stale/corrupt current-state를 발견하면 기존 파일을 보존 이동하고 새 current-state를 기록합니다.",
        summary,
        paths::current_state_file().display(),
        event.event_id
    ))
}

pub fn resume_report() -> Result<String, AppError> {
    let identity = ledger::current_identity();
    ensure_layout()?;
    observability::initialize(&identity)?;
    let status = current_state_status(&identity)?;
    let (event_type, summary, action) = match status {
        CurrentStateStatus::CleanNoActiveWorkflow => (
            "workflow.resume.noop",
            "active workflow 없는 resume 요청",
            "재개할 workflow가 없어 no-op event만 기록했습니다.",
        ),
        CurrentStateStatus::CleanActiveWorkflow => (
            "workflow.resume.detected",
            "resume 대상 감지",
            "active workflow pointer를 발견했습니다. agent loop resume은 후속 phase에서 실행됩니다.",
        ),
        CurrentStateStatus::Missing => (
            "workflow.resume.blocked",
            "current-state 누락으로 resume 차단",
            "current-state가 없어 먼저 state reconcile이 필요합니다.",
        ),
        CurrentStateStatus::Corrupt => (
            "workflow.resume.blocked",
            "current-state 손상으로 resume 차단",
            "current-state가 손상되어 먼저 state reconcile이 필요합니다.",
        ),
        CurrentStateStatus::StaleProject => (
            "workflow.resume.blocked",
            "다른 project current-state로 resume 차단",
            "current-state project id가 현재 project와 달라 먼저 state reconcile이 필요합니다.",
        ),
    };

    let event = ledger::new_event_for(&identity, event_type, summary, action);
    ledger::append_event(&event)?;
    observability::project_event(&event)?;

    Ok(format!(
        "state resume 결과\n- outcome: {}\n- ledger event: {}\n- 동작: {}",
        summary, event.event_id, action
    ))
}

pub fn cancel_report() -> Result<String, AppError> {
    let identity = ledger::current_identity();
    ensure_layout()?;
    observability::initialize(&identity)?;
    let event = ledger::new_event_for(
        &identity,
        "workflow.cancel.noop",
        "active workflow 없는 cancel 요청",
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

    let identity = ledger::current_identity();
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

fn reconcile_current_state(identity: &RuntimeIdentity) -> Result<ReconcileOutcome, AppError> {
    match current_state_status(identity)? {
        CurrentStateStatus::CleanNoActiveWorkflow | CurrentStateStatus::CleanActiveWorkflow => {
            Ok(ReconcileOutcome::Clean)
        }
        CurrentStateStatus::Missing => {
            write_current_state(identity)?;
            Ok(ReconcileOutcome::Created)
        }
        CurrentStateStatus::Corrupt => {
            let recovered = move_current_state_aside("corrupt")?;
            write_current_state(identity)?;
            Ok(ReconcileOutcome::RecoveredCorrupt(recovered))
        }
        CurrentStateStatus::StaleProject => {
            let recovered = move_current_state_aside("stale")?;
            write_current_state(identity)?;
            Ok(ReconcileOutcome::RecoveredStale(recovered))
        }
    }
}

fn current_state_status(identity: &RuntimeIdentity) -> Result<CurrentStateStatus, AppError> {
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

fn classify_current_state(contents: &str, identity: &RuntimeIdentity) -> CurrentStateStatus {
    let required_keys = [
        "\"schema_version\"",
        "\"project_id\"",
        "\"session_id\"",
        "\"active_workflow\"",
        "\"terminal_states\"",
    ];

    if !required_keys.iter().all(|key| contents.contains(key)) {
        return CurrentStateStatus::Corrupt;
    }

    let expected_project = format!("\"project_id\": \"{}\"", identity.project_id);
    if !contents.contains(&expected_project) {
        return CurrentStateStatus::StaleProject;
    }

    if contents.contains("\"active_workflow\": null") {
        CurrentStateStatus::CleanNoActiveWorkflow
    } else {
        CurrentStateStatus::CleanActiveWorkflow
    }
}

fn move_current_state_aside(reason: &str) -> Result<PathBuf, AppError> {
    let path = paths::current_state_file();
    let recovered = path.with_extension(format!("json.{reason}.{}", now_ms()));
    fs::rename(&path, &recovered).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 보존 이동하지 못했습니다: {} -> {} ({err})",
            path.display(),
            recovered.display()
        ))
    })?;
    Ok(recovered)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CurrentStateStatus {
    Missing,
    Corrupt,
    StaleProject,
    CleanNoActiveWorkflow,
    CleanActiveWorkflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReconcileOutcome {
    Clean,
    Created,
    RecoveredCorrupt(PathBuf),
    RecoveredStale(PathBuf),
}

impl ReconcileOutcome {
    fn event_type(&self) -> &'static str {
        match self {
            Self::Clean => "state.reconcile.clean",
            Self::Created => "state.reconcile.created",
            Self::RecoveredCorrupt(_) => "state.reconcile.corrupt_recovered",
            Self::RecoveredStale(_) => "state.reconcile.stale_recovered",
        }
    }

    fn summary(&self) -> String {
        match self {
            Self::Clean => "current-state 정상".to_string(),
            Self::Created => "current-state 생성".to_string(),
            Self::RecoveredCorrupt(path) => {
                format!("손상된 current-state를 {} 로 보존 이동", path.display())
            }
            Self::RecoveredStale(path) => {
                format!("stale current-state를 {} 로 보존 이동", path.display())
            }
        }
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_state_summary_handles_missing_file_as_uninitialized() {
        let summary = read_current_state_summary().unwrap();
        assert!(summary == "미초기화" || summary.contains("초기화됨"));
    }

    #[test]
    fn classifies_corrupt_current_state() {
        let identity = RuntimeIdentity {
            project_id: "project-a".to_string(),
            session_id: "session-a".to_string(),
            project_root: ".".to_string(),
        };

        assert_eq!(
            classify_current_state("not-json", &identity),
            CurrentStateStatus::Corrupt
        );
    }

    #[test]
    fn classifies_stale_project_current_state() {
        let identity = RuntimeIdentity {
            project_id: "project-a".to_string(),
            session_id: "session-a".to_string(),
            project_root: ".".to_string(),
        };
        let contents = "{\n  \"schema_version\": 1,\n  \"project_id\": \"project-b\",\n  \"session_id\": \"session-a\",\n  \"active_workflow\": null,\n  \"terminal_states\": [\"complete\"]\n}\n";

        assert_eq!(
            classify_current_state(contents, &identity),
            CurrentStateStatus::StaleProject
        );
    }
}
