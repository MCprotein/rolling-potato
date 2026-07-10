use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::AppError;
use crate::ledger::{self, RuntimeIdentity};
use crate::observability::SessionHistoryEntry;
use crate::observability::{self, StoreStatus};
use crate::paths;
use sha2::{Digest, Sha256};

const WORKFLOW_SCHEMA_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRecord {
    pub workflow_id: String,
    pub revision: u64,
    pub previous_hash: String,
    pub artifact_hash: String,
    pub project_id: String,
    pub session_id: String,
    pub phase: String,
    pub request_hash: String,
    pub action_id: String,
    pub action_status: String,
    pub source_path: String,
    pub source_hash: String,
    pub find_text: String,
    pub replace_text: String,
    pub proposal_id: String,
    pub proposal_hash: String,
    pub approval_credential_hash: String,
    pub before_hash: String,
    pub after_hash: String,
    pub verification_plan: String,
    pub approval_state: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    pub failure_reason: String,
}

impl WorkflowRecord {
    pub fn new(request: &str) -> Self {
        let identity = ledger::current_identity();
        let nonce = format!("{}\n{}\n{}", identity.session_id, request, now_ms());
        let workflow_id = format!("workflow-{}", &sha256_text(&nonce)[..20]);
        Self {
            action_id: format!(
                "action-{}",
                &sha256_text(&format!("{workflow_id}\naction"))[..20]
            ),
            workflow_id,
            revision: 0,
            previous_hash: "none".to_string(),
            artifact_hash: String::new(),
            project_id: identity.project_id,
            session_id: identity.session_id,
            phase: "model-pending".to_string(),
            request_hash: sha256_text(request),
            action_status: "runtime-candidate".to_string(),
            source_path: String::new(),
            source_hash: String::new(),
            find_text: String::new(),
            replace_text: String::new(),
            proposal_id: String::new(),
            proposal_hash: String::new(),
            approval_credential_hash: String::new(),
            before_hash: String::new(),
            after_hash: String::new(),
            verification_plan: String::new(),
            approval_state: "not-requested".to_string(),
            evidence_id: String::new(),
            evidence_hash: String::new(),
            failure_reason: String::new(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.phase.as_str(), "complete" | "failed" | "cancelled")
    }
}

pub fn create_workflow(request: &str) -> Result<WorkflowRecord, AppError> {
    ensure_layout()?;
    ledger::validated_current_identity()?;
    let record = WorkflowRecord::new(request);
    checkpoint_workflow(record, 0)
}

pub fn checkpoint_workflow(
    mut next: WorkflowRecord,
    expected_revision: u64,
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(&next.workflow_id)?;
    let _workflow_lock = crate::lease::RecoverableLease::acquire(
        paths::project_workflows_dir().join(format!("{}.checkpoint.lock", next.workflow_id)),
        "workflow checkpoint",
    )?;
    recover_workflow_transaction(&next.workflow_id)?;
    let pointer_path = paths::project_workflow_file(&next.workflow_id);
    if expected_revision == 0 {
        if pointer_path.exists() {
            return Err(AppError::blocked(format!(
                "workflow 저장 차단\n- 이유: 동일 workflow artifact가 이미 존재합니다.\n- workflow id: {}",
                next.workflow_id
            )));
        }
        next.revision = 1;
        next.previous_hash = "none".to_string();
    } else {
        let current = load_workflow(&next.workflow_id)?;
        if current.revision != expected_revision || current.artifact_hash != next.artifact_hash {
            return Err(AppError::blocked(format!(
                "workflow 저장 차단\n- 이유: revision/hash conflict\n- workflow id: {}\n- expected revision: {}\n- current revision: {}",
                next.workflow_id, expected_revision, current.revision
            )));
        }
        next.previous_hash = current.artifact_hash;
        next.revision = expected_revision + 1;
    }
    next.artifact_hash = sha256_text(&workflow_payload(&next));
    write_workflow_transaction(&next)?;
    checkpoint_fault("after-transaction")?;
    write_workflow_snapshot(&next)?;
    checkpoint_fault("after-snapshot")?;

    let identity = RuntimeIdentity {
        project_id: next.project_id.clone(),
        session_id: next.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    };
    let event = ledger::new_event_for(
        &identity,
        "workflow.checkpoint",
        "canonical workflow revision persisted",
        &format!(
            "workflow_id={} revision={} artifact_hash={} previous_hash={} phase={} action_id={} proposal_id={} evidence_id={}",
            next.workflow_id,
            next.revision,
            next.artifact_hash,
            next.previous_hash,
            next.phase,
            next.action_id,
            display_empty(&next.proposal_id),
            display_empty(&next.evidence_id)
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)?;
    checkpoint_fault("after-ledger")?;
    write_workflow_pointer(&next)?;
    checkpoint_fault("after-pointer")?;
    remove_workflow_transaction(&next.workflow_id)?;
    write_current_state_for_session(&identity, None, Some(&next.workflow_id))?;
    Ok(next)
}

pub fn load_workflow(workflow_id: &str) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(workflow_id)?;
    recover_workflow_transaction(workflow_id)?;
    let pointer_path = paths::project_workflow_file(workflow_id);
    let pointer = fs::read_to_string(&pointer_path).map_err(|err| {
        AppError::blocked(format!(
            "workflow 읽기 차단\n- 이유: committed workflow pointer를 읽지 못했습니다.\n- workflow id: {workflow_id}\n- path: {}\n- error: {err}",
            pointer_path.display()
        ))
    })?;
    let context = pointer_path.display().to_string();
    let object = crate::strict_json::parse_object(
        &pointer,
        &[
            "schema_version",
            "artifact_version",
            "workflow_id",
            "committed_revision",
            "artifact_hash",
        ],
        &context,
    )
    .map_err(|_| corrupt_workflow(&pointer_path))?;
    if crate::strict_json::number(&object, "schema_version", &context)
        .map_err(|_| corrupt_workflow(&pointer_path))?
        != WORKFLOW_SCHEMA_VERSION
        || crate::strict_json::string(&object, "artifact_version", &context)
            .map_err(|_| corrupt_workflow(&pointer_path))?
            != "workflow-commit-v1"
    {
        return Err(corrupt_workflow(&pointer_path));
    }
    let pointer_workflow = crate::strict_json::string(&object, "workflow_id", &context)
        .map_err(|_| corrupt_workflow(&pointer_path))?;
    let revision = crate::strict_json::number(&object, "committed_revision", &context)
        .map_err(|_| corrupt_workflow(&pointer_path))?;
    let pointer_hash = crate::strict_json::string(&object, "artifact_hash", &context)
        .map_err(|_| corrupt_workflow(&pointer_path))?;
    if pointer_workflow != workflow_id || revision == 0 {
        return Err(corrupt_workflow(&pointer_path));
    }
    let record = validate_workflow_chain(workflow_id, revision)?;
    let identity = ledger::validated_current_identity()?;
    if record.artifact_hash != pointer_hash || record.project_id != identity.project_id {
        return Err(corrupt_workflow(&pointer_path));
    }
    Ok(record)
}

pub fn active_workflow_id() -> Result<Option<String>, AppError> {
    let discovered = discover_active_workflow()?;
    let path = paths::current_state_file();
    if !path.exists() {
        if let Some(workflow_id) = discovered.as_deref() {
            let workflow = load_workflow(workflow_id)?;
            write_current_state_for_session(
                &workflow_identity(&workflow),
                Some("workflow-pointer-recovery"),
                Some(workflow_id),
            )?;
        }
        return Ok(discovered);
    }
    let body = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "current-state를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;
    let object = crate::strict_json::parse_object(
        &body,
        &[
            "schema_version",
            "project_id",
            "project_root",
            "session_id",
            "active_workflow",
            "parent_session_id",
            "branch_from_event_id",
            "compaction_boundary",
            "resume_source",
            "terminal_states",
        ],
        "current-state",
    )?;
    if crate::strict_json::number(&object, "schema_version", "current-state")? != 1
        || crate::strict_json::string(&object, "project_id", "current-state")?
            != ledger::fresh_identity().project_id
        || !matches!(
            object.get("terminal_states"),
            Some(crate::strict_json::Value::Array(_))
        )
    {
        return Err(AppError::blocked(
            "current-state schema/project binding이 손상되었습니다.",
        ));
    }
    let pointer = match object.get("active_workflow") {
        Some(crate::strict_json::Value::Null) => None,
        Some(crate::strict_json::Value::String(value)) => Some(value.clone()),
        _ => {
            return Err(AppError::blocked(
                "current-state active_workflow pointer가 손상되었습니다.",
            ))
        }
    };
    match (pointer, discovered) {
        (None, None) => Ok(None),
        (None, Some(workflow_id)) => {
            let workflow = load_workflow(&workflow_id)?;
            write_current_state_for_session(
                &workflow_identity(&workflow),
                Some("workflow-pointer-recovery"),
                Some(&workflow_id),
            )?;
            Ok(Some(workflow_id))
        }
        (Some(pointer), Some(workflow_id)) if pointer == workflow_id => Ok(Some(workflow_id)),
        (Some(pointer), None) => {
            let workflow = load_workflow(&pointer)?;
            if !workflow.is_terminal() {
                return Err(AppError::blocked(
                    "workflow resume 차단\n- 이유: current pointer와 전체 artifact scan이 충돌합니다.",
                ));
            }
            Ok(Some(pointer))
        }
        _ => Err(AppError::blocked(
            "workflow resume 차단\n- 이유: current pointer와 non-terminal artifact가 충돌합니다.\n- 동작: fail-closed; backend와 side effect를 실행하지 않습니다.",
        )),
    }
}

pub(crate) fn clear_terminal_workflow_pointer(workflow: &WorkflowRecord) -> Result<(), AppError> {
    if !workflow.is_terminal() {
        return Err(AppError::blocked(
            "terminal workflow pointer cleanup 차단: workflow가 terminal이 아닙니다.",
        ));
    }
    let path = paths::current_state_file();
    let body = fs::read_to_string(&path).map_err(|err| {
        AppError::blocked(format!("terminal pointer current-state 읽기 실패: {err}"))
    })?;
    let object = crate::strict_json::parse_object(
        &body,
        &[
            "schema_version",
            "project_id",
            "project_root",
            "session_id",
            "active_workflow",
            "parent_session_id",
            "branch_from_event_id",
            "compaction_boundary",
            "resume_source",
            "terminal_states",
        ],
        "current-state",
    )?;
    match object.get("active_workflow") {
        Some(crate::strict_json::Value::String(value)) if value == &workflow.workflow_id => {}
        Some(crate::strict_json::Value::Null) => return Ok(()),
        _ => {
            return Err(AppError::blocked(
                "terminal workflow pointer cleanup 차단: current pointer conflict",
            ))
        }
    }
    write_current_state_for_session(
        &workflow_identity(workflow),
        Some("terminal-pointer-cleanup"),
        None,
    )
}

fn discover_active_workflow() -> Result<Option<String>, AppError> {
    let entries = match fs::read_dir(paths::project_workflows_dir()) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AppError::runtime(format!(
                "workflow directory를 읽지 못했습니다: {err}"
            )))
        }
    };
    let mut workflow_ids = std::collections::BTreeSet::new();
    for entry in entries {
        let path = entry
            .map_err(|err| AppError::runtime(format!("workflow entry read 실패: {err}")))?
            .path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| corrupt_workflow(&path))?;
        let workflow_id = name
            .strip_suffix(".json")
            .or_else(|| name.strip_suffix(".txn"))
            .or_else(|| name.strip_suffix(".snapshots"));
        let Some(workflow_id) = workflow_id else {
            continue;
        };
        validate_workflow_id(workflow_id)?;
        workflow_ids.insert(workflow_id.to_string());
    }
    let mut active = Vec::new();
    for workflow_id in workflow_ids {
        if !paths::project_workflow_file(&workflow_id).exists()
            && !paths::project_workflow_transaction_file(&workflow_id).exists()
        {
            return Err(AppError::blocked(format!(
                "workflow scan 차단\n- 이유: committed pointer와 transaction이 없는 snapshot artifact\n- workflow id: {workflow_id}"
            )));
        }
        let workflow = load_workflow(&workflow_id)?;
        if !workflow.is_terminal() {
            active.push(workflow.workflow_id);
        }
    }
    match active.as_slice() {
        [] => Ok(None),
        [workflow_id] => Ok(Some(workflow_id.clone())),
        _ => Err(AppError::blocked(
            "workflow resume 차단\n- 이유: 여러 non-terminal canonical workflow가 충돌합니다.\n- 동작: fail-closed; backend와 side effect를 실행하지 않습니다.",
        )),
    }
}

pub fn sha256_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateInit {
    pub identity: RuntimeIdentity,
    pub created_paths: Vec<PathBuf>,
    pub store: StoreStatus,
}

pub fn initialize() -> Result<StateInit, AppError> {
    let identity = ledger::validated_current_identity()?;
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
    let active = active_workflow_id()?.unwrap_or_else(|| "없음".to_string());
    let current_state = read_current_state_summary()?;
    let store = observability::status()?;
    let recovered = store
        .recovered_from
        .as_ref()
        .map(|path| format!("\n- recovered corrupt db: {}", path.display()))
        .unwrap_or_default();

    Ok(format!(
        "state 상태\n- app state dir: {}\n- project state dir: {}\n- runtime ledger: {}\n- project session ledger: {}\n- current state: {}\n- observability db: {}\n- schema migration: v{}\n- ledger events: {}\n- sessions: {}\n- workflows: {}\n- active workflow: {}\n- transcript parent/branch pointer: current-state schema에 null로 보존\n- evidence stale policy: {}{}",
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
        active,
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
    ensure_layout()?;
    if let Some(workflow_id) = active_workflow_id()? {
        return crate::patch::resume_workflow_report(&workflow_id);
    }
    let identity = ledger::current_identity();
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
    ensure_layout()?;
    if let Some(workflow_id) = active_workflow_id()? {
        return crate::patch::cancel_workflow_report(&workflow_id);
    }
    let identity = ledger::current_identity();
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

pub fn session_list_report() -> Result<String, AppError> {
    let identity = ledger::current_identity();
    ensure_layout()?;
    let sessions = observability::session_history(20)?;
    if sessions.is_empty() {
        return Ok(format!(
            "session history\n- project: {}\n- sessions: 없음\n- 다음 단계: `rpotato init` 또는 `rpotato session new`로 세션을 시작하세요.",
            identity.project_root
        ));
    }

    let rows = sessions
        .iter()
        .map(format_session_row)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        "session history\n- project: {}\n- current session: {}\n- resume: `rpotato session resume <session-id>` 또는 `rpotato resume <session-id>`\n{}",
        identity.project_root, identity.session_id, rows
    ))
}

pub fn session_new_report() -> Result<String, AppError> {
    ensure_layout()?;
    let identity = ledger::fresh_identity();
    write_current_state(&identity)?;
    ensure_runtime_evidence_file()?;
    observability::initialize(&identity)?;
    let event = ledger::new_event_for(
        &identity,
        "session.new",
        "새 session 시작",
        "session history에 새 resume target 등록",
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)?;

    Ok(format!(
        "session new 결과\n- session id: {}\n- current state: {}\n- ledger event: {}\n- 동작: 이후 명령은 이 session id로 ledger와 SQLite projection에 이어 기록됩니다.",
        identity.session_id,
        paths::current_state_file().display(),
        event.event_id
    ))
}

pub fn session_resume_report(session_id: &str) -> Result<String, AppError> {
    ensure_layout()?;
    let Some(session) = observability::session_entry(session_id)? else {
        return Err(AppError::blocked(format!(
            "session resume 차단\n- session id: {}\n- 이유: 현재 project의 SQLite session history에서 찾지 못했습니다.\n- 확인: `rpotato session list`",
            session_id
        )));
    };

    let resumed = RuntimeIdentity {
        project_id: session.project_id.clone(),
        session_id: session.session_id.clone(),
        project_root: session.project_root.clone(),
    };
    write_current_state_for_session(&resumed, Some("session-history"), None)?;
    let event = ledger::new_event_for(
        &resumed,
        "session.resume.selected",
        "session history에서 resume target 선택",
        &format!("selected_session_id={}", session.session_id),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)?;

    Ok(format!(
        "session resume 결과\n- selected session: {}\n- events: {}\n- last event: {}\n- current state: {}\n- ledger event: {}\n- 동작: 이후 명령은 선택한 session id로 이어 기록됩니다. 실제 agent loop 재개는 backend/agent phase에서 이 current-state를 사용합니다.",
        session.session_id,
        session.event_count,
        session.last_summary.unwrap_or_else(|| "없음".to_string()),
        paths::current_state_file().display(),
        event.event_id
    ))
}

pub fn record_event(event_type: &str, summary: &str, details: &str) -> Result<String, AppError> {
    let identity = ledger::current_identity();
    ensure_layout()?;
    observability::initialize(&identity)?;
    let event = ledger::new_event_for(&identity, event_type, summary, details);
    let event_id = event.event_id.clone();
    ledger::append_event(&event)?;
    observability::project_event(&event)?;
    Ok(event_id)
}

pub fn workflow_ownership_summary() -> &'static str {
    "active workflow는 current-state가 소유하고 skill/plugin/TUI는 parent workflow pointer를 받아야 합니다."
}

fn ensure_layout() -> Result<Vec<PathBuf>, AppError> {
    let directories = [
        paths::config_dir(),
        paths::backends_dir(),
        paths::models_dir(),
        paths::model_registry_dir(),
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
        paths::project_approval_requests_dir(),
        paths::project_workflows_dir(),
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
    write_current_state_for_session(identity, None, None)
}

fn write_current_state_for_session(
    identity: &RuntimeIdentity,
    resume_source: Option<&str>,
    active_workflow: Option<&str>,
) -> Result<(), AppError> {
    let resume_source = resume_source
        .map(|source| format!("\"{}\"", ledger::json_string(source)))
        .unwrap_or_else(|| "null".to_string());
    let active_workflow = active_workflow
        .map(|value| format!("\"{}\"", ledger::json_string(value)))
        .unwrap_or_else(|| "null".to_string());
    let body = format!(
        "{{\n  \"schema_version\": 1,\n  \"project_id\": \"{}\",\n  \"project_root\": \"{}\",\n  \"session_id\": \"{}\",\n  \"active_workflow\": {},\n  \"parent_session_id\": null,\n  \"branch_from_event_id\": null,\n  \"compaction_boundary\": null,\n  \"resume_source\": {},\n  \"terminal_states\": [\"complete\", \"failed\", \"cancelled\"]\n}}\n",
        ledger::json_string(&identity.project_id),
        ledger::json_string(&identity.project_root),
        ledger::json_string(&identity.session_id),
        active_workflow,
        resume_source
    );

    atomic_replace_bytes(&paths::current_state_file(), body.as_bytes())
}

fn format_session_row(session: &SessionHistoryEntry) -> String {
    let last_event = session
        .last_event_at_ms
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string());
    let summary = session.last_summary.as_deref().unwrap_or("이벤트 없음");

    format!(
        "- {} | started {} | last {} | events {} | {}",
        session.session_id, session.started_at_ms, last_event, session.event_count, summary
    )
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
    let Ok(object) = crate::strict_json::parse_object(
        contents,
        &[
            "schema_version",
            "project_id",
            "project_root",
            "session_id",
            "active_workflow",
            "parent_session_id",
            "branch_from_event_id",
            "compaction_boundary",
            "resume_source",
            "terminal_states",
        ],
        "current-state classification",
    ) else {
        return CurrentStateStatus::Corrupt;
    };
    if crate::strict_json::number(&object, "schema_version", "current-state classification").ok()
        != Some(1)
        || crate::strict_json::string(&object, "session_id", "current-state classification")
            .is_err()
        || !matches!(
            object.get("terminal_states"),
            Some(crate::strict_json::Value::Array(_))
        )
    {
        return CurrentStateStatus::Corrupt;
    }
    let Ok(project_id) =
        crate::strict_json::string(&object, "project_id", "current-state classification")
    else {
        return CurrentStateStatus::Corrupt;
    };
    if project_id != identity.project_id {
        return CurrentStateStatus::StaleProject;
    }
    match object.get("active_workflow") {
        Some(crate::strict_json::Value::Null) => CurrentStateStatus::CleanNoActiveWorkflow,
        Some(crate::strict_json::Value::String(_)) => CurrentStateStatus::CleanActiveWorkflow,
        _ => CurrentStateStatus::Corrupt,
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

fn workflow_payload(record: &WorkflowRecord) -> String {
    format!(
        "schema_version={WORKFLOW_SCHEMA_VERSION}\nworkflow_id={}\nrevision={}\nprevious_hash={}\nproject_id={}\nsession_id={}\nphase={}\nrequest_hash={}\naction_id={}\naction_status={}\nsource_path={}\nsource_hash={}\nfind_text={}\nreplace_text={}\nproposal_id={}\nproposal_hash={}\napproval_credential_hash={}\nbefore_hash={}\nafter_hash={}\nverification_plan={}\napproval_state={}\nevidence_id={}\nevidence_hash={}\nfailure_reason={}\n",
        record.workflow_id,
        record.revision,
        record.previous_hash,
        record.project_id,
        record.session_id,
        record.phase,
        record.request_hash,
        record.action_id,
        record.action_status,
        record.source_path,
        record.source_hash,
        record.find_text,
        record.replace_text,
        record.proposal_id,
        record.proposal_hash,
        record.approval_credential_hash,
        record.before_hash,
        record.after_hash,
        record.verification_plan,
        record.approval_state,
        record.evidence_id,
        record.evidence_hash,
        record.failure_reason
    )
}

fn render_workflow(record: &WorkflowRecord) -> String {
    format!(
        concat!(
            "{{\n",
            "  \"schema_version\": {},\n",
            "  \"artifact_version\": \"workflow-v1\",\n",
            "  \"workflow_id\": \"{}\",\n",
            "  \"revision\": {},\n",
            "  \"previous_hash\": \"{}\",\n",
            "  \"artifact_hash\": \"{}\",\n",
            "  \"project_id\": \"{}\",\n",
            "  \"session_id\": \"{}\",\n",
            "  \"phase\": \"{}\",\n",
            "  \"request_hash\": \"{}\",\n",
            "  \"action_id\": \"{}\",\n",
            "  \"action_status\": \"{}\",\n",
            "  \"source_path\": \"{}\",\n",
            "  \"source_hash\": \"{}\",\n",
            "  \"find_text\": \"{}\",\n",
            "  \"replace_text\": \"{}\",\n",
            "  \"proposal_id\": \"{}\",\n",
            "  \"proposal_hash\": \"{}\",\n",
            "  \"approval_credential_hash\": \"{}\",\n",
            "  \"before_hash\": \"{}\",\n",
            "  \"after_hash\": \"{}\",\n",
            "  \"verification_plan\": \"{}\",\n",
            "  \"approval_state\": \"{}\",\n",
            "  \"evidence_id\": \"{}\",\n",
            "  \"evidence_hash\": \"{}\",\n",
            "  \"failure_reason\": \"{}\"\n",
            "}}\n"
        ),
        WORKFLOW_SCHEMA_VERSION,
        ledger::json_string(&record.workflow_id),
        record.revision,
        ledger::json_string(&record.previous_hash),
        ledger::json_string(&record.artifact_hash),
        ledger::json_string(&record.project_id),
        ledger::json_string(&record.session_id),
        ledger::json_string(&record.phase),
        ledger::json_string(&record.request_hash),
        ledger::json_string(&record.action_id),
        ledger::json_string(&record.action_status),
        ledger::json_string(&record.source_path),
        ledger::json_string(&record.source_hash),
        ledger::json_string(&record.find_text),
        ledger::json_string(&record.replace_text),
        ledger::json_string(&record.proposal_id),
        ledger::json_string(&record.proposal_hash),
        ledger::json_string(&record.approval_credential_hash),
        ledger::json_string(&record.before_hash),
        ledger::json_string(&record.after_hash),
        ledger::json_string(&record.verification_plan),
        ledger::json_string(&record.approval_state),
        ledger::json_string(&record.evidence_id),
        ledger::json_string(&record.evidence_hash),
        ledger::json_string(&record.failure_reason)
    )
}

fn write_workflow_transaction(record: &WorkflowRecord) -> Result<(), AppError> {
    atomic_replace_bytes(
        &paths::project_workflow_transaction_file(&record.workflow_id),
        render_workflow(record).as_bytes(),
    )
}

fn write_workflow_snapshot(record: &WorkflowRecord) -> Result<(), AppError> {
    let path = paths::project_workflow_snapshot_file(&record.workflow_id, record.revision);
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("workflow parent path 없음"))?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "workflow directory를 만들지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;
    let rendered = render_workflow(record);
    if path.exists() {
        let existing = fs::read(&path).map_err(|err| {
            AppError::runtime(format!(
                "workflow snapshot read 실패: {} ({err})",
                path.display()
            ))
        })?;
        if existing == rendered.as_bytes() {
            return Ok(());
        }
        return Err(AppError::blocked(format!(
            "workflow snapshot overwrite 차단\n- path: {}\n- 이유: immutable revision bytes conflict",
            path.display()
        )));
    }
    atomic_replace_bytes(&path, rendered.as_bytes())
}

fn write_workflow_pointer(record: &WorkflowRecord) -> Result<(), AppError> {
    let body = format!(
        "{{\n  \"schema_version\": 1,\n  \"artifact_version\": \"workflow-commit-v1\",\n  \"workflow_id\": \"{}\",\n  \"committed_revision\": {},\n  \"artifact_hash\": \"{}\"\n}}\n",
        ledger::json_string(&record.workflow_id),
        record.revision,
        record.artifact_hash
    );
    atomic_replace_bytes(
        &paths::project_workflow_file(&record.workflow_id),
        body.as_bytes(),
    )
}

fn remove_workflow_transaction(workflow_id: &str) -> Result<(), AppError> {
    let path = paths::project_workflow_transaction_file(workflow_id);
    match fs::remove_file(&path) {
        Ok(()) => sync_parent(&path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::runtime(format!(
            "workflow transaction cleanup 실패: {} ({err})",
            path.display()
        ))),
    }
}

fn recover_workflow_transaction(workflow_id: &str) -> Result<(), AppError> {
    let transaction_path = paths::project_workflow_transaction_file(workflow_id);
    if !transaction_path.exists() {
        return Ok(());
    }
    let body = fs::read_to_string(&transaction_path).map_err(|err| {
        AppError::blocked(format!(
            "workflow recovery 차단\n- 이유: transaction을 읽지 못했습니다.\n- path: {}\n- error: {err}",
            transaction_path.display()
        ))
    })?;
    let record = parse_workflow_snapshot(&transaction_path, &body)?;
    if record.workflow_id != workflow_id {
        return Err(corrupt_workflow(&transaction_path));
    }
    let pointer_path = paths::project_workflow_file(workflow_id);
    if pointer_path.exists() {
        let pointer =
            fs::read_to_string(&pointer_path).map_err(|_| corrupt_workflow(&pointer_path))?;
        let context = pointer_path.display().to_string();
        let object = crate::strict_json::parse_object(
            &pointer,
            &[
                "schema_version",
                "artifact_version",
                "workflow_id",
                "committed_revision",
                "artifact_hash",
            ],
            &context,
        )
        .map_err(|_| corrupt_workflow(&pointer_path))?;
        if crate::strict_json::number(&object, "schema_version", &context)
            .map_err(|_| corrupt_workflow(&pointer_path))?
            != WORKFLOW_SCHEMA_VERSION
            || crate::strict_json::string(&object, "artifact_version", &context)
                .map_err(|_| corrupt_workflow(&pointer_path))?
                != "workflow-commit-v1"
        {
            return Err(corrupt_workflow(&pointer_path));
        }
        let committed = crate::strict_json::number(&object, "committed_revision", &context)
            .map_err(|_| corrupt_workflow(&pointer_path))?;
        let committed_hash = crate::strict_json::string(&object, "artifact_hash", &context)
            .map_err(|_| corrupt_workflow(&pointer_path))?;
        if committed == record.revision && committed_hash == record.artifact_hash {
            remove_workflow_transaction(workflow_id)?;
            return Ok(());
        }
        if committed + 1 != record.revision {
            return Err(corrupt_workflow(&transaction_path));
        }
    } else if record.revision != 1 {
        return Err(corrupt_workflow(&transaction_path));
    }

    write_workflow_snapshot(&record)?;
    if !ledger::workflow_checkpoint_exists(workflow_id, record.revision, &record.artifact_hash)? {
        append_workflow_checkpoint_event(&record)?;
    }
    write_workflow_pointer(&record)?;
    remove_workflow_transaction(workflow_id)
}

fn append_workflow_checkpoint_event(record: &WorkflowRecord) -> Result<(), AppError> {
    let identity = workflow_identity(record);
    let event = ledger::new_event_for(
        &identity,
        "workflow.checkpoint",
        "canonical workflow revision persisted",
        &format!(
            "workflow_id={} revision={} artifact_hash={} previous_hash={} phase={} action_id={} proposal_id={} evidence_id={}",
            record.workflow_id,
            record.revision,
            record.artifact_hash,
            record.previous_hash,
            record.phase,
            record.action_id,
            display_empty(&record.proposal_id),
            display_empty(&record.evidence_id)
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

fn validate_workflow_chain(
    workflow_id: &str,
    committed_revision: u64,
) -> Result<WorkflowRecord, AppError> {
    let checkpoints = ledger::workflow_checkpoints(workflow_id)?;
    if checkpoints.len() != committed_revision as usize {
        return Err(AppError::blocked(format!(
            "workflow chain 검증 차단\n- workflow id: {workflow_id}\n- committed revision: {committed_revision}\n- ledger checkpoints: {}",
            checkpoints.len()
        )));
    }
    let mut previous_hash = "none".to_string();
    let mut latest = None;
    for revision in 1..=committed_revision {
        let path = paths::project_workflow_snapshot_file(workflow_id, revision);
        let body = fs::read_to_string(&path).map_err(|err| {
            AppError::blocked(format!(
                "workflow chain 검증 차단\n- 이유: revision snapshot 누락\n- path: {}\n- error: {err}",
                path.display()
            ))
        })?;
        let record = parse_workflow_snapshot(&path, &body)?;
        let checkpoint = &checkpoints[(revision - 1) as usize];
        if record.workflow_id != workflow_id
            || record.revision != revision
            || record.previous_hash != previous_hash
            || checkpoint.revision != revision
            || checkpoint.artifact_hash != record.artifact_hash
            || checkpoint.previous_hash != previous_hash
        {
            return Err(corrupt_workflow(&path));
        }
        previous_hash = record.artifact_hash.clone();
        latest = Some(record);
    }
    latest.ok_or_else(|| corrupt_workflow(&paths::project_workflow_file(workflow_id)))
}

fn parse_workflow_snapshot(path: &std::path::Path, body: &str) -> Result<WorkflowRecord, AppError> {
    const KEYS: &[&str] = &[
        "schema_version",
        "artifact_version",
        "workflow_id",
        "revision",
        "previous_hash",
        "artifact_hash",
        "project_id",
        "session_id",
        "phase",
        "request_hash",
        "action_id",
        "action_status",
        "source_path",
        "source_hash",
        "find_text",
        "replace_text",
        "proposal_id",
        "proposal_hash",
        "approval_credential_hash",
        "before_hash",
        "after_hash",
        "verification_plan",
        "approval_state",
        "evidence_id",
        "evidence_hash",
        "failure_reason",
    ];
    let context = path.display().to_string();
    let object = crate::strict_json::parse_object(body, KEYS, &context)
        .map_err(|_| corrupt_workflow(path))?;
    let schema = crate::strict_json::number(&object, "schema_version", &context)
        .map_err(|_| corrupt_workflow(path))?;
    if schema != WORKFLOW_SCHEMA_VERSION {
        return Err(corrupt_workflow(path));
    }
    let text = |key| {
        crate::strict_json::string(&object, key, &context).map_err(|_| corrupt_workflow(path))
    };
    if text("artifact_version")? != "workflow-v1" {
        return Err(corrupt_workflow(path));
    }
    let record = WorkflowRecord {
        workflow_id: text("workflow_id")?,
        revision: crate::strict_json::number(&object, "revision", &context)
            .map_err(|_| corrupt_workflow(path))?,
        previous_hash: text("previous_hash")?,
        artifact_hash: text("artifact_hash")?,
        project_id: text("project_id")?,
        session_id: text("session_id")?,
        phase: text("phase")?,
        request_hash: text("request_hash")?,
        action_id: text("action_id")?,
        action_status: text("action_status")?,
        source_path: text("source_path")?,
        source_hash: text("source_hash")?,
        find_text: text("find_text")?,
        replace_text: text("replace_text")?,
        proposal_id: text("proposal_id")?,
        proposal_hash: text("proposal_hash")?,
        approval_credential_hash: text("approval_credential_hash")?,
        before_hash: text("before_hash")?,
        after_hash: text("after_hash")?,
        verification_plan: text("verification_plan")?,
        approval_state: text("approval_state")?,
        evidence_id: text("evidence_id")?,
        evidence_hash: text("evidence_hash")?,
        failure_reason: text("failure_reason")?,
    };
    if record.artifact_hash != sha256_text(&workflow_payload(&record)) {
        return Err(corrupt_workflow(path));
    }
    Ok(record)
}

fn workflow_identity(record: &WorkflowRecord) -> RuntimeIdentity {
    RuntimeIdentity {
        project_id: record.project_id.clone(),
        session_id: record.session_id.clone(),
        project_root: paths::project_root().display().to_string(),
    }
}

pub(crate) fn atomic_replace_bytes(path: &std::path::Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("atomic write parent path 없음"))?;
    fs::create_dir_all(parent).map_err(|err| {
        AppError::runtime(format!(
            "atomic write directory 생성 실패: {} ({err})",
            parent.display()
        ))
    })?;
    let temporary = path.with_extension(format!("tmp.{}.{}", std::process::id(), now_ms()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(|err| {
        AppError::runtime(format!(
            "atomic temp 생성 실패: {} ({err})",
            temporary.display()
        ))
    })?;
    if let Ok(metadata) = fs::metadata(path) {
        file.set_permissions(metadata.permissions())
            .map_err(|err| AppError::runtime(format!("atomic temp permission 복사 실패: {err}")))?;
    }
    file.write_all(bytes)
        .map_err(|err| AppError::runtime(format!("atomic temp write 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("atomic temp sync 실패: {err}")))?;
    drop(file);
    replace_file(&temporary, path).map_err(|err| {
        let _ = fs::remove_file(&temporary);
        AppError::runtime(format!(
            "atomic replace 실패: {} -> {} ({err})",
            temporary.display(),
            path.display()
        ))
    })?;
    sync_parent(path)
}

pub(crate) fn guarded_source_replace(
    target: &std::path::Path,
    expected_current_hash: &str,
    replacement: &[u8],
    expected_replacement_hash: &str,
    transaction_path: &std::path::Path,
) -> Result<(), AppError> {
    recover_source_replace(target, transaction_path)?;
    let parent = target
        .parent()
        .ok_or_else(|| AppError::runtime("guarded source parent path 없음"))?;
    let nonce = format!("{}.{}", std::process::id(), now_ms());
    let guard = parent.join(format!(
        ".{}.rpotato-guard-{nonce}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("source")
    ));
    let temporary = parent.join(format!(
        ".{}.rpotato-new-{nonce}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("source")
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|err| AppError::runtime(format!("guarded source temp 생성 실패: {err}")))?;
    if let Ok(metadata) = fs::metadata(target) {
        file.set_permissions(metadata.permissions())
            .map_err(|err| {
                AppError::runtime(format!("guarded source permission 복사 실패: {err}"))
            })?;
    }
    file.write_all(replacement)
        .map_err(|err| AppError::runtime(format!("guarded source temp write 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("guarded source temp sync 실패: {err}")))?;
    drop(file);
    if sha256_bytes(
        &fs::read(&temporary)
            .map_err(|err| AppError::runtime(format!("guarded source temp reread 실패: {err}")))?,
    ) != expected_replacement_hash
    {
        let _ = fs::remove_file(&temporary);
        return Err(AppError::blocked("guarded source replacement hash 불일치"));
    }
    let txn = format!(
        "target={}\nguard={}\ntemporary={}\nexpected_current_hash={}\nexpected_replacement_hash={}\n",
        target.display(), guard.display(), temporary.display(), expected_current_hash, expected_replacement_hash
    );
    atomic_replace_bytes(transaction_path, txn.as_bytes())?;
    fs::rename(target, &guard)
        .map_err(|err| AppError::blocked(format!("guarded source 기존 target move 실패: {err}")))?;
    sync_parent(target)?;
    let guarded_hash =
        sha256_bytes(&fs::read(&guard).map_err(|err| {
            AppError::blocked(format!("guarded source bytes reread 실패: {err}"))
        })?);
    if guarded_hash != expected_current_hash {
        restore_guard_without_clobber(target, &guard)?;
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(transaction_path);
        return Err(AppError::blocked(format!(
            "guarded source conflict\n- expected: {expected_current_hash}\n- moved bytes: {guarded_hash}"
        )));
    }
    if let Err(err) = source_replace_fault("after-guard") {
        recover_source_replace(target, transaction_path)?;
        return Err(err);
    }
    if let Err(err) = fs::hard_link(&temporary, target) {
        let restore = restore_guard_without_clobber(target, &guard);
        return Err(AppError::blocked(format!(
            "guarded source install 차단: destination이 다시 생성되었거나 install에 실패했습니다 ({err}); restore={}",
            restore.map(|_| "ok").unwrap_or("conflict")
        )));
    }
    sync_parent(target)?;
    if let Err(err) = source_replace_fault("after-install") {
        recover_source_replace(target, transaction_path)?;
        return Err(err);
    }
    let installed_hash = sha256_bytes(&fs::read(target).map_err(|err| {
        AppError::blocked(format!("guarded source installed reread 실패: {err}"))
    })?);
    if installed_hash != expected_replacement_hash {
        return Err(AppError::blocked("guarded source installed hash 불일치"));
    }
    fs::remove_file(&temporary)
        .map_err(|err| AppError::runtime(format!("guarded temp cleanup 실패: {err}")))?;
    fs::remove_file(&guard)
        .map_err(|err| AppError::runtime(format!("guarded original cleanup 실패: {err}")))?;
    fs::remove_file(transaction_path)
        .map_err(|err| AppError::runtime(format!("guarded transaction cleanup 실패: {err}")))?;
    sync_parent(target)
}

fn restore_guard_without_clobber(
    target: &std::path::Path,
    guard: &std::path::Path,
) -> Result<(), AppError> {
    fs::hard_link(guard, target).map_err(|err| {
        AppError::blocked(format!(
            "guarded source restore conflict; 외부 target을 덮어쓰지 않았습니다: {err}; guard={}",
            guard.display()
        ))
    })?;
    fs::remove_file(guard)
        .map_err(|err| AppError::runtime(format!("guarded source restore cleanup 실패: {err}")))?;
    sync_parent(target)
}

fn recover_source_replace(
    target: &std::path::Path,
    transaction_path: &std::path::Path,
) -> Result<(), AppError> {
    if !transaction_path.exists() {
        return Ok(());
    }
    let body = fs::read_to_string(transaction_path)
        .map_err(|err| AppError::blocked(format!("source transaction 읽기 실패: {err}")))?;
    let fields = body.lines().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(AppError::blocked("source transaction schema 손상"));
    }
    let exact = |index: usize, key: &str| -> Result<&str, AppError> {
        fields[index]
            .strip_prefix(key)
            .ok_or_else(|| AppError::blocked("source transaction schema 손상"))
    };
    let recorded_target = std::path::PathBuf::from(exact(0, "target=")?);
    let guard = std::path::PathBuf::from(exact(1, "guard=")?);
    let temporary = std::path::PathBuf::from(exact(2, "temporary=")?);
    let original_hash = exact(3, "expected_current_hash=")?;
    let replacement_hash = exact(4, "expected_replacement_hash=")?;
    if recorded_target != target || !is_sha256(original_hash) || !is_sha256(replacement_hash) {
        return Err(AppError::blocked("source transaction binding 손상"));
    }
    if target.exists() {
        let hash = sha256_bytes(&fs::read(target).map_err(|err| {
            AppError::blocked(format!("source transaction target reread 실패: {err}"))
        })?);
        if hash != original_hash && hash != replacement_hash {
            return Err(AppError::blocked(
                "source transaction recovery conflict; 외부 source를 덮어쓰지 않았습니다.",
            ));
        }
    } else if guard.exists() {
        restore_guard_without_clobber(target, &guard)?;
    } else {
        return Err(AppError::blocked("source transaction recovery bytes 누락"));
    }
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|err| {
            AppError::runtime(format!("source recovery temp cleanup 실패: {err}"))
        })?;
    }
    if guard.exists() {
        fs::remove_file(&guard).map_err(|err| {
            AppError::runtime(format!("source recovery guard cleanup 실패: {err}"))
        })?;
    }
    fs::remove_file(transaction_path)
        .map_err(|err| AppError::runtime(format!("source recovery txn cleanup 실패: {err}")))?;
    sync_parent(target)
}

fn source_replace_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_SOURCE_REPLACE_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected source replacement fault: {point}"
        )));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(not(windows))]
fn replace_file(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn replace_file(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    type Bool = i32;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> Bool;
    }
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both pointers reference NUL-terminated buffers that remain alive for the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn sync_parent(path: &std::path::Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::runtime("sync parent path 없음"))?;
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|err| {
            AppError::runtime(format!(
                "parent directory sync 실패: {} ({err})",
                parent.display()
            ))
        })
}

#[cfg(windows)]
fn sync_parent(_path: &std::path::Path) -> Result<(), AppError> {
    Ok(())
}

fn checkpoint_fault(point: &str) -> Result<(), AppError> {
    if cfg!(debug_assertions)
        && std::env::var("RPOTATO_TEST_CHECKPOINT_FAULT").as_deref() == Ok(point)
    {
        return Err(AppError::runtime(format!(
            "injected checkpoint fault: {point}"
        )));
    }
    Ok(())
}

fn validate_workflow_id(workflow_id: &str) -> Result<(), AppError> {
    if workflow_id.starts_with("workflow-")
        && workflow_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        Ok(())
    } else {
        Err(AppError::blocked("workflow id 형식이 안전하지 않습니다."))
    }
}

fn corrupt_workflow(path: &std::path::Path) -> AppError {
    let persistence = record_validation_gap("corrupt-workflow", &path.display().to_string())
        .err()
        .map(|err| format!("\n- validation-gap 저장 실패: {}", err.message))
        .unwrap_or_default();
    AppError::blocked(format!(
        "workflow 읽기 차단\n- 이유: canonical workflow artifact가 손상되었거나 ledger checkpoint와 충돌합니다.\n- path: {}\n- 동작: fail-closed; backend와 side effect를 실행하지 않습니다.{}",
        path.display(), persistence
    ))
}

pub fn record_validation_gap(kind: &str, artifact: &str) -> Result<(), AppError> {
    let path = paths::validation_gaps_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AppError::runtime(format!("validation gap directory 생성 실패: {err}"))
        })?;
    }
    let line = format!(
        "{{\"schema_version\":1,\"kind\":\"{}\",\"artifact_hash\":\"{}\",\"recorded_at_ms\":{}}}",
        ledger::json_string(kind),
        sha256_text(artifact),
        now_ms()
    );
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|err| AppError::runtime(format!("validation gap open 실패: {err}")))?;
    writeln!(file, "{line}")
        .map_err(|err| AppError::runtime(format!("validation gap append 실패: {err}")))?;
    file.sync_all()
        .map_err(|err| AppError::runtime(format!("validation gap sync 실패: {err}")))
}

fn display_empty(value: &str) -> &str {
    if value.is_empty() {
        "none"
    } else {
        value
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

    fn workflow_test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rpotato-{name}-{}-{}",
            std::process::id(),
            now_ms()
        ))
    }

    fn with_workflow_env<T>(name: &str, test: impl FnOnce(&PathBuf) -> T) -> T {
        let root = workflow_test_root(name);
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
        initialize().unwrap();
        let result = test(&root);
        std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);
        result
    }

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

    #[test]
    fn session_list_does_not_create_current_state_when_history_is_empty() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-session-list-empty-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let report = session_list_report().unwrap();
        let current_state_exists = paths::current_state_file().exists();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert!(report.contains("sessions: 없음"));
        assert!(!current_state_exists);
    }

    #[test]
    fn session_resume_selects_existing_history_entry() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-session-resume-test-{}",
            std::process::id()
        ));
        let project_root = root.join("project");
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);

        let new_report = session_new_report().unwrap();
        let session_id = new_report
            .lines()
            .find_map(|line| line.strip_prefix("- session id: "))
            .unwrap()
            .to_string();
        let list_report = session_list_report().unwrap();
        let resume_report = session_resume_report(&session_id).unwrap();
        let current_state = fs::read_to_string(paths::current_state_file()).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");

        assert!(list_report.contains(&session_id));
        assert!(resume_report.contains("session resume 결과"));
        assert!(current_state.contains(&format!("\"session_id\": \"{session_id}\"")));
        assert!(current_state.contains("\"resume_source\": \"session-history\""));
    }

    #[test]
    fn checkpoint_crash_windows_recover_one_committed_revision() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        for point in [
            "after-transaction",
            "after-snapshot",
            "after-ledger",
            "after-pointer",
        ] {
            with_workflow_env(point, |_| {
                std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", point);
                let error = create_workflow("recover me").unwrap_err();
                assert!(error.message.contains("injected checkpoint fault"));
                std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");

                let workflow_id = active_workflow_id().unwrap().unwrap();
                let workflow = load_workflow(&workflow_id).unwrap();
                let checkpoints = ledger::workflow_checkpoints(&workflow_id).unwrap();
                assert_eq!(workflow.revision, 1, "fault point: {point}");
                assert_eq!(checkpoints.len(), 1, "fault point: {point}");
                assert!(!paths::project_workflow_transaction_file(&workflow_id).exists());
            });
        }
    }

    #[test]
    fn terminal_pointer_cleanup_revalidates_stop_gate_before_clear() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("terminal-pointer-cleanup", |_| {
            let mut workflow = create_workflow("finish me").unwrap();
            workflow.phase = "complete".to_string();
            std::env::set_var("RPOTATO_TEST_CHECKPOINT_FAULT", "after-pointer");
            checkpoint_workflow(workflow.clone(), workflow.revision).unwrap_err();
            std::env::remove_var("RPOTATO_TEST_CHECKPOINT_FAULT");

            assert_eq!(
                active_workflow_id().unwrap(),
                Some(workflow.workflow_id.clone())
            );
            let error = resume_report().unwrap_err();
            assert!(error.message.contains("proposal"));
            let current = fs::read_to_string(paths::current_state_file()).unwrap();
            assert!(current.contains(&workflow.workflow_id));
            assert!(load_workflow(&workflow.workflow_id).unwrap().is_terminal());
        });
    }

    #[test]
    fn all_artifacts_are_scanned_and_multiple_active_workflows_fail_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("multi-active", |_| {
            let first = create_workflow("first").unwrap();
            let second = create_workflow("second").unwrap();
            assert_ne!(first.workflow_id, second.workflow_id);

            let error = active_workflow_id().unwrap_err();
            assert_eq!(error.code, 3);
            assert!(error.message.contains("여러 non-terminal"));
        });
    }

    #[test]
    fn state_status_reports_the_discovered_active_workflow() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("status-active", |_| {
            let workflow = create_workflow("status truth").unwrap();
            let report = status_report().unwrap();
            assert!(report.contains(&format!("active workflow: {}", workflow.workflow_id)));
        });
    }

    #[test]
    fn snapshot_tamper_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("snapshot-tamper", |_| {
            let workflow = create_workflow("tamper me").unwrap();
            let snapshot = paths::project_workflow_snapshot_file(&workflow.workflow_id, 1);
            let mut body = fs::read_to_string(&snapshot).unwrap();
            body = body.replace("model-pending", "approved");
            fs::write(&snapshot, body).unwrap();

            let error = load_workflow(&workflow.workflow_id).unwrap_err();
            assert_eq!(error.code, 3);
            assert!(error.message.contains("fail-closed"));
        });
    }

    #[test]
    fn ledger_ahead_of_committed_pointer_fails_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        with_workflow_env("ledger-ahead", |_| {
            let workflow = create_workflow("stale latest checkpoint").unwrap();
            let identity = workflow_identity(&workflow);
            let forged_hash = "d".repeat(64);
            let event = ledger::new_event_for(
                &identity,
                "workflow.checkpoint",
                "forged uncommitted checkpoint",
                &format!(
                    "workflow_id={} revision=2 artifact_hash={forged_hash} previous_hash={} phase=approved action_id={} proposal_id=none evidence_id=none",
                    workflow.workflow_id, workflow.artifact_hash, workflow.action_id
                ),
            );
            ledger::append_event(&event).unwrap();

            let error = load_workflow(&workflow.workflow_id).unwrap_err();
            assert_eq!(error.code, 3);
            assert!(error.message.contains("ledger checkpoints: 2"));
        });
    }
}
