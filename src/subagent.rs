use crate::app::AppError;
use crate::{backend, lease, ledger, paths, state, strict_json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;
pub const DEFAULT_MAX_TOKENS: u32 = 256;
pub const MAX_MAX_TOKENS: u32 = 1_024;
pub const MAX_TASK_BYTES: usize = 4_096;
pub const MAX_DECLARED_PATHS: usize = 4;
const SUBAGENT_SCHEMA_VERSION: u64 = 1;
const MAX_RECORD_REVISIONS: u64 = 4;
const MAX_SUBAGENT_RECORDS: usize = 256;
static SUBAGENT_ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const RECORD_KEYS: &[&str] = &[
    "schema_version",
    "subagent_id",
    "revision",
    "previous_hash",
    "artifact_hash",
    "project_id",
    "session_id",
    "parent_workflow_id",
    "parent_revision",
    "parent_artifact_hash",
    "role",
    "task_hash",
    "declared_tools",
    "read_paths",
    "write_paths",
    "timeout_ms",
    "requested_max_tokens",
    "effective_max_tokens",
    "status",
    "backend_event_id",
    "result_artifact_id",
    "result_artifact_hash",
    "evidence_id",
    "evidence_hash",
    "failure_code",
    "created_at_ms",
    "started_at_ms",
    "finished_at_ms",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubagentRole {
    Explore,
    Planner,
    Verifier,
    Critic,
    Writer,
    Executor,
}

impl SubagentRole {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "explore" => Some(Self::Explore),
            "planner" => Some(Self::Planner),
            "verifier" => Some(Self::Verifier),
            "critic" => Some(Self::Critic),
            "writer" => Some(Self::Writer),
            "executor" => Some(Self::Executor),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Planner => "planner",
            Self::Verifier => "verifier",
            Self::Critic => "critic",
            Self::Writer => "writer",
            Self::Executor => "executor",
        }
    }

    fn allows(self, tool: SubagentTool) -> bool {
        tool == SubagentTool::ReadFile
            || (self == Self::Executor && tool == SubagentTool::RenderDiff)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubagentTool {
    ReadFile,
    RenderDiff,
}

impl SubagentTool {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read_file" => Some(Self::ReadFile),
            "render_diff" => Some(Self::RenderDiff),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadFile => "read_file",
            Self::RenderDiff => "render_diff",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    Requested,
    Admitted,
    Running,
    Completed,
    Blocked,
    Failed,
    Cancelled,
    TimedOut,
}

impl SubagentStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "requested" => Some(Self::Requested),
            "admitted" => Some(Self::Admitted),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "blocked" => Some(Self::Blocked),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "timed-out" => Some(Self::TimedOut),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Admitted => "admitted",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed-out",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Blocked | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }

    fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Requested,
                Self::Admitted | Self::Blocked | Self::Cancelled
            ) | (Self::Admitted, Self::Running | Self::Cancelled)
                | (
                    Self::Running,
                    Self::Completed
                        | Self::Blocked
                        | Self::Failed
                        | Self::Cancelled
                        | Self::TimedOut
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedLaunch {
    pub role: SubagentRole,
    pub task_hash: String,
    pub declared_tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub requested_max_tokens: u32,
}

#[derive(Debug)]
struct AdmittedLaunch {
    record: SubagentRecordV1,
    context: crate::context::ContextPack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkerGeneration {
    backend_event_id: String,
    effective_max_tokens: u32,
    response: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentRecordV1 {
    pub subagent_id: String,
    pub revision: u64,
    pub previous_hash: String,
    pub artifact_hash: String,
    pub project_id: String,
    pub session_id: String,
    pub parent_workflow_id: String,
    pub parent_revision: u64,
    pub parent_artifact_hash: String,
    pub role: SubagentRole,
    pub task_hash: String,
    pub declared_tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub requested_max_tokens: u32,
    pub effective_max_tokens: u32,
    pub status: SubagentStatus,
    pub backend_event_id: String,
    pub result_artifact_id: String,
    pub result_artifact_hash: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    pub failure_code: String,
    pub created_at_ms: u128,
    pub started_at_ms: u128,
    pub finished_at_ms: u128,
}

pub fn validate_launch(
    role: &str,
    task: &str,
    declared_tools: &[String],
    read_paths: &[String],
    write_paths: &[String],
    timeout_ms: Option<u32>,
    max_tokens: Option<u32>,
) -> Result<ValidatedLaunch, AppError> {
    let role = SubagentRole::parse(role)
        .ok_or_else(|| AppError::usage(format!("지원하지 않는 subagent role입니다: {role}")))?;
    let task = task.trim();
    if task.is_empty() || task.len() > MAX_TASK_BYTES {
        return Err(AppError::usage(format!(
            "subagent task는 1..={MAX_TASK_BYTES} UTF-8 byte 범위여야 합니다."
        )));
    }
    if declared_tools.is_empty() {
        return Err(AppError::usage(
            "subagent launch는 최소 하나의 --tool 선언이 필요합니다.",
        ));
    }
    let tools = normalize_tools(role, declared_tools)?;
    let read_paths = normalize_paths("read", read_paths, true)?;
    let write_paths = normalize_paths("write", write_paths, false)?;
    let has_render_diff = tools.iter().any(|tool| tool == "render_diff");
    if role != SubagentRole::Executor && !write_paths.is_empty() {
        return Err(AppError::blocked(
            "executor가 아닌 subagent role은 write ownership을 선언할 수 없습니다.",
        ));
    }
    if has_render_diff != !write_paths.is_empty() {
        return Err(AppError::blocked(
            "render_diff tool과 하나 이상의 write path는 함께 선언해야 합니다.",
        ));
    }
    if write_paths.iter().any(|owner| {
        !read_paths.iter().any(|read| {
            read == owner
                || read
                    .strip_prefix(owner)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    }) {
        return Err(AppError::blocked(
            "subagent write ownership이 declared read target과 겹치지 않습니다.",
        ));
    }
    let timeout_ms = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    if timeout_ms == 0 || timeout_ms > backend::MAX_CHAT_TIMEOUT_MS {
        return Err(AppError::usage(format!(
            "subagent timeout은 1..={} ms 범위여야 합니다.",
            backend::MAX_CHAT_TIMEOUT_MS
        )));
    }
    let requested_max_tokens = max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    if requested_max_tokens == 0 || requested_max_tokens > MAX_MAX_TOKENS {
        return Err(AppError::usage(format!(
            "subagent max tokens는 1..={MAX_MAX_TOKENS} 범위여야 합니다."
        )));
    }
    Ok(ValidatedLaunch {
        role,
        task_hash: state::sha256_text(task),
        declared_tools: tools,
        read_paths,
        write_paths,
        timeout_ms,
        requested_max_tokens,
    })
}

impl SubagentRecordV1 {
    pub fn new(
        project_id: &str,
        session_id: &str,
        parent_workflow_id: &str,
        parent_revision: u64,
        parent_artifact_hash: &str,
        launch: ValidatedLaunch,
    ) -> Result<Self, AppError> {
        let created_at_ms = now_ms()?;
        let nonce = format!(
            "{project_id}\n{session_id}\n{parent_workflow_id}\n{}\n{created_at_ms}\n{}\n{}",
            launch.task_hash,
            std::process::id(),
            SUBAGENT_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let record = Self {
            subagent_id: format!("subagent-{}", &state::sha256_text(&nonce)[..20]),
            revision: 0,
            previous_hash: String::new(),
            artifact_hash: String::new(),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            parent_workflow_id: parent_workflow_id.to_string(),
            parent_revision,
            parent_artifact_hash: parent_artifact_hash.to_string(),
            role: launch.role,
            task_hash: launch.task_hash,
            declared_tools: launch.declared_tools,
            read_paths: launch.read_paths,
            write_paths: launch.write_paths,
            timeout_ms: launch.timeout_ms,
            requested_max_tokens: launch.requested_max_tokens,
            effective_max_tokens: launch.requested_max_tokens,
            status: SubagentStatus::Requested,
            backend_event_id: String::new(),
            result_artifact_id: String::new(),
            result_artifact_hash: String::new(),
            evidence_id: String::new(),
            evidence_hash: String::new(),
            failure_code: String::new(),
            created_at_ms,
            started_at_ms: 0,
            finished_at_ms: 0,
        };
        validate_record(&record, false)?;
        Ok(record)
    }

    pub fn transition_to(
        &mut self,
        next: SubagentStatus,
        failure_code: Option<&str>,
    ) -> Result<(), AppError> {
        if !self.status.permits(next) {
            return Err(AppError::blocked(format!(
                "subagent 상태 전이 차단\n- current: {}\n- next: {}",
                self.status.as_str(),
                next.as_str()
            )));
        }
        let timestamp = now_ms()?;
        if next == SubagentStatus::Running {
            self.started_at_ms = timestamp;
        }
        if next.is_terminal() {
            self.finished_at_ms = timestamp.max(self.started_at_ms);
        }
        self.failure_code = failure_code.unwrap_or("").trim().to_string();
        self.status = next;
        Ok(())
    }
}

pub fn create_record(record: SubagentRecordV1) -> Result<SubagentRecordV1, AppError> {
    checkpoint_record(record, 0)
}

pub fn checkpoint_record(
    mut next: SubagentRecordV1,
    expected_revision: u64,
) -> Result<SubagentRecordV1, AppError> {
    validate_subagent_id(&next.subagent_id)?;
    let _lease = lease::RecoverableLease::acquire(
        paths::project_subagent_lock(&next.subagent_id),
        "subagent state",
    )?;
    let current_path = paths::project_subagent_file(&next.subagent_id);
    if expected_revision == 0 {
        if current_path.exists() {
            return Err(AppError::blocked(format!(
                "subagent create 충돌\n- subagent id: {}",
                next.subagent_id
            )));
        }
        if next.revision != 0 || !next.artifact_hash.is_empty() {
            return Err(AppError::blocked(
                "새 subagent record는 revision 0과 빈 artifact hash에서 시작해야 합니다.",
            ));
        }
        next.revision = 1;
        next.previous_hash = "none".to_string();
    } else {
        let current = load_record_unlocked(&next.subagent_id)?;
        if current.revision != expected_revision
            || next.revision != current.revision
            || next.artifact_hash != current.artifact_hash
        {
            return Err(AppError::blocked(format!(
                "subagent stale revision 차단\n- expected: {expected_revision}\n- actual: {}",
                current.revision
            )));
        }
        if !current.status.permits(next.status) {
            return Err(AppError::blocked(format!(
                "subagent 상태 전이 차단\n- current: {}\n- next: {}",
                current.status.as_str(),
                next.status.as_str()
            )));
        }
        if immutable_binding_changed(&current, &next) {
            return Err(AppError::blocked(
                "subagent immutable launch binding 변경 차단",
            ));
        }
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("subagent revision overflow"))?;
        next.previous_hash = current.artifact_hash;
    }
    if next.revision > MAX_RECORD_REVISIONS {
        return Err(AppError::blocked("subagent lifecycle revision 상한 초과"));
    }
    next.artifact_hash = state::sha256_text(&render_payload(&next));
    validate_record(&next, true)?;
    let body = render_record(&next);
    install_snapshot(&next, &body)?;
    state::atomic_replace_bytes(&current_path, body.as_bytes())?;
    let installed = load_record_unlocked(&next.subagent_id)?;
    if installed != next {
        return Err(AppError::blocked(
            "subagent canonical state install 검증 실패",
        ));
    }
    Ok(installed)
}

pub fn load_record(subagent_id: &str) -> Result<SubagentRecordV1, AppError> {
    validate_subagent_id(subagent_id)?;
    let path = paths::project_subagent_file(subagent_id);
    let before = fs::read_to_string(&path).map_err(|err| {
        AppError::blocked(format!(
            "subagent state 읽기 차단\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    let record = parse_record(&path, &before)?;
    verify_snapshot_chain(&record, &before)?;
    let after = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("subagent state 재확인 실패: {err}")))?;
    if after != before {
        return Err(AppError::blocked(
            "subagent state가 read 중 변경되어 결과를 폐기합니다.",
        ));
    }
    Ok(record)
}

pub fn launch_report(
    role: &str,
    task: &str,
    declared_tools: &[String],
    read_paths: &[String],
    write_paths: &[String],
    timeout_ms: Option<u32>,
    max_tokens: Option<u32>,
) -> Result<String, AppError> {
    let launch = validate_launch(
        role,
        task,
        declared_tools,
        read_paths,
        write_paths,
        timeout_ms,
        max_tokens,
    )?;
    backend::preflight_chat_ready()?;
    let admitted = admit_launch(launch)?;
    let completed = dispatch_admitted(admitted, task, |prompt, max_tokens, timeout_ms| {
        let run = backend::chat_once_bounded(prompt, max_tokens, timeout_ms)?;
        Ok(WorkerGeneration {
            backend_event_id: run.ledger_event,
            effective_max_tokens: run.effective_max_tokens,
            response: run.response,
        })
    })?;
    let record = &completed.record;
    Ok(format!(
        "subagent launch\n- status: {}\n- subagent id: {}\n- parent workflow: {}\n- parent revision: {}\n- role: {}\n- task hash: {}\n- tools: {}\n- read paths: {}\n- write paths: {}\n- timeout ms: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- context origin: {}\n- context files: {}\n- context chars: {}\n- source pointers: {}\n- result artifact: {}\n- evidence: {}\n- summary: {}\n- boundary: child output was strictly parsed and recorded; no command or patch was executed.",
        record.status.as_str(),
        record.subagent_id,
        record.parent_workflow_id,
        record.parent_revision,
        record.role.as_str(),
        record.task_hash,
        record.declared_tools.join(", "),
        record.read_paths.join(", "),
        display_list(&record.write_paths),
        record.timeout_ms,
        record.requested_max_tokens,
        record.effective_max_tokens,
        completed.context.origin,
        completed.context.files_read,
        completed.context.chars_read,
        completed.context.pointer_summary(),
        record.result_artifact_id,
        record.evidence_id,
        completed.summary,
    ))
}

pub fn status_report(subagent_id: Option<&str>) -> Result<String, AppError> {
    let record = match subagent_id {
        Some(subagent_id) => load_record(subagent_id)?,
        None => latest_active_parent_record()?,
    };
    Ok(render_status_report(&record, "read-only"))
}

pub fn cancel_report(subagent_id: &str) -> Result<String, AppError> {
    let initial = load_record(subagent_id)?;
    let identity = ledger::validated_current_identity()?;
    if initial.project_id != identity.project_id || initial.session_id != identity.session_id {
        return Err(AppError::blocked(
            "subagent cancel owner binding 불일치\n- 동작: 다른 project/session child를 변경하지 않았습니다.",
        ));
    }
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(&initial.parent_workflow_id),
        "subagent parent admission",
    )?;
    let current = load_record(subagent_id)?;
    if current.status == SubagentStatus::Cancelled {
        return Ok(render_status_report(&current, "already-cancelled-no-op"));
    }
    if current.status.is_terminal() {
        return Ok(render_status_report(&current, "terminal-preserved-no-op"));
    }
    if current.status == SubagentStatus::Running {
        backend::cancel_generation_report()?;
    }
    let mut cancelled = current.clone();
    cancelled.transition_to(SubagentStatus::Cancelled, Some("user-cancelled"))?;
    let cancelled = checkpoint_record(cancelled, current.revision)?;
    append_lifecycle_event(
        &identity,
        &cancelled,
        "team.subagent.cancelled",
        "subagent cancelled",
    )?;
    Ok(render_status_report(&cancelled, "cancelled"))
}

#[derive(Debug)]
struct CompletedLaunch {
    record: SubagentRecordV1,
    context: crate::context::ContextPack,
    summary: String,
}

fn dispatch_admitted(
    admitted: AdmittedLaunch,
    task: &str,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedLaunch, AppError> {
    let _execution_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_execution_lock(&admitted.record.subagent_id),
        "subagent execution",
    )?;
    let (running, context) = prepare_running(&admitted)?;
    let prompt = render_worker_prompt(&running, task, &context);
    let generation = match runner(&prompt, running.requested_max_tokens, running.timeout_ms) {
        Ok(generation) => generation,
        Err(error) => {
            let terminal = terminalize_running_error(&running, &error)?;
            return Err(AppError {
                code: error.code,
                message: format!(
                    "{}\n- subagent id: {}\n- subagent status: {}\n- partial output: discarded",
                    error.message,
                    terminal.subagent_id,
                    terminal.status.as_str()
                ),
            });
        }
    };
    complete_generation(running, context, generation)
}

fn prepare_running(
    admitted: &AdmittedLaunch,
) -> Result<(SubagentRecordV1, crate::context::ContextPack), AppError> {
    let record = &admitted.record;
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(&record.parent_workflow_id),
        "subagent parent admission",
    )?;
    if state::active_workflow_id()?.as_deref() != Some(record.parent_workflow_id.as_str()) {
        return Err(AppError::blocked(
            "subagent dispatch 차단: active parent pointer 변경",
        ));
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(&record.parent_workflow_id)?;
    let parent = workflow_guard.load_current()?;
    if parent.is_terminal()
        || parent.revision != record.parent_revision
        || parent.artifact_hash != record.parent_artifact_hash
        || parent.project_id != record.project_id
        || parent.session_id != record.session_id
    {
        return Err(AppError::blocked(
            "subagent dispatch 차단: exact parent binding 변경",
        ));
    }
    let current = load_record(&record.subagent_id)?;
    if current != *record || current.status != SubagentStatus::Admitted {
        return Err(AppError::blocked(
            "subagent dispatch 차단: admitted state binding 변경",
        ));
    }
    let context =
        crate::context::verify_declared_context_pack(&admitted.context, &current.read_paths)?;
    let mut running = current.clone();
    running.transition_to(SubagentStatus::Running, None)?;
    let running = checkpoint_record(running, current.revision)?;
    append_lifecycle_event(
        &ledger::validated_current_identity()?,
        &running,
        "team.subagent.started",
        "subagent started",
    )?;
    Ok((running, context))
}

fn complete_generation(
    running: SubagentRecordV1,
    context: crate::context::ContextPack,
    generation: WorkerGeneration,
) -> Result<CompletedLaunch, AppError> {
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(&running.parent_workflow_id),
        "subagent parent admission",
    )?;
    let current = load_record(&running.subagent_id)?;
    if current.status == SubagentStatus::Cancelled {
        return Err(AppError::blocked(format!(
            "subagent completion 폐기\n- subagent id: {}\n- 이유: cancellation이 먼저 terminal state를 획득했습니다.",
            current.subagent_id
        )));
    }
    if current != running || current.status != SubagentStatus::Running {
        return Err(AppError::blocked(
            "subagent completion 차단: running state binding 변경",
        ));
    }
    let parent = state::load_workflow(&running.parent_workflow_id)?;
    if parent.is_terminal()
        || parent.revision != running.parent_revision
        || parent.artifact_hash != running.parent_artifact_hash
        || parent.project_id != running.project_id
        || parent.session_id != running.session_id
    {
        let blocked = terminalize_locked(
            &current,
            SubagentStatus::Blocked,
            "stale-parent",
            "team.subagent.blocked",
        )?;
        return Err(AppError::blocked(format!(
            "subagent result merge 차단: stale parent\n- subagent id: {}\n- status: {}",
            blocked.subagent_id,
            blocked.status.as_str()
        )));
    }
    let context = match crate::context::verify_declared_context_pack(&context, &running.read_paths)
    {
        Ok(context) => context,
        Err(error) => {
            terminalize_locked(
                &current,
                SubagentStatus::Blocked,
                "stale-context",
                "team.subagent.blocked",
            )?;
            return Err(error);
        }
    };
    if generation.effective_max_tokens == 0
        || generation.effective_max_tokens > running.requested_max_tokens
        || generation.backend_event_id.is_empty()
    {
        terminalize_locked(
            &current,
            SubagentStatus::Blocked,
            "invalid-backend-metadata",
            "team.subagent.blocked",
        )?;
        return Err(AppError::blocked("subagent backend metadata binding 오류"));
    }
    let stored =
        match crate::subagent_result::parse_and_store(&running, &context, &generation.response) {
            Ok(stored) => stored,
            Err(error) => {
                terminalize_locked(
                    &current,
                    SubagentStatus::Blocked,
                    "invalid-result",
                    "team.subagent.blocked",
                )?;
                return Err(AppError::blocked(format!(
                    "subagent result 검증 차단\n- subagent id: {}\n- reason: {}",
                    running.subagent_id, error.message
                )));
            }
        };
    crate::subagent_result::verify_stored_artifacts(&running, &stored)?;
    let mut completed = current.clone();
    completed.backend_event_id = generation.backend_event_id;
    completed.effective_max_tokens = generation.effective_max_tokens;
    completed.result_artifact_id = stored.result_artifact_id.clone();
    completed.result_artifact_hash = stored.result_artifact_hash.clone();
    completed.evidence_id = stored.evidence_id.clone();
    completed.evidence_hash = stored.evidence_hash.clone();
    completed.transition_to(SubagentStatus::Completed, None)?;
    let completed = checkpoint_record(completed, current.revision)?;
    let identity = ledger::validated_current_identity()?;
    append_lifecycle_event(
        &identity,
        &completed,
        "team.subagent.completed",
        "subagent completed",
    )?;
    merge_completed_result(&completed)?;
    Ok(CompletedLaunch {
        record: completed,
        context,
        summary: stored.result.summary,
    })
}

fn terminalize_running_error(
    running: &SubagentRecordV1,
    error: &AppError,
) -> Result<SubagentRecordV1, AppError> {
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(&running.parent_workflow_id),
        "subagent parent admission",
    )?;
    let current = load_record(&running.subagent_id)?;
    if current.status.is_terminal() {
        return Ok(current);
    }
    if current != *running || current.status != SubagentStatus::Running {
        return Err(AppError::blocked(
            "subagent backend failure state binding 변경",
        ));
    }
    let (status, failure_code, event_type) = classify_backend_error(error);
    terminalize_locked(&current, status, failure_code, event_type)
}

fn terminalize_locked(
    current: &SubagentRecordV1,
    status: SubagentStatus,
    failure_code: &str,
    event_type: &str,
) -> Result<SubagentRecordV1, AppError> {
    let mut terminal = current.clone();
    terminal.transition_to(status, Some(failure_code))?;
    let terminal = checkpoint_record(terminal, current.revision)?;
    append_lifecycle_event(
        &ledger::validated_current_identity()?,
        &terminal,
        event_type,
        "subagent terminal failure",
    )?;
    Ok(terminal)
}

fn classify_backend_error(error: &AppError) -> (SubagentStatus, &'static str, &'static str) {
    if error.message.contains("제한 시간 초과") || error.message.contains("timed-out") {
        (
            SubagentStatus::TimedOut,
            "backend-timeout",
            "team.subagent.timed-out",
        )
    } else if error.message.contains("취소됨") || error.message.contains("cancelled") {
        (
            SubagentStatus::Cancelled,
            "backend-cancelled",
            "team.subagent.cancelled",
        )
    } else if error.code == 3 {
        (
            SubagentStatus::Blocked,
            "backend-blocked",
            "team.subagent.blocked",
        )
    } else {
        (
            SubagentStatus::Failed,
            "backend-failed",
            "team.subagent.failed",
        )
    }
}

fn merge_completed_result(completed: &SubagentRecordV1) -> Result<(), AppError> {
    crate::subagent_result::verify_completed_artifacts(completed)?;
    let parent = state::load_workflow(&completed.parent_workflow_id)?;
    if parent.project_id != completed.project_id || parent.session_id != completed.session_id {
        return Err(AppError::blocked(
            "subagent parent merge owner binding 불일치",
        ));
    }
    let parent_has_evidence = workflow_has_evidence(&parent, &completed.evidence_id);
    let prior_merge = ledger::read_runtime_events()?.into_iter().find(|event| {
        event.event_type == "team.subagent.result-merged"
            && detail_token(&event.details, "subagent_id") == Some(completed.subagent_id.as_str())
    });
    if let Some(prior) = prior_merge {
        if detail_token(&prior.details, "result_hash")
            == Some(completed.result_artifact_hash.as_str())
            && parent_has_evidence
        {
            return Ok(());
        }
        return Err(AppError::blocked(
            "subagent second different result merge 차단",
        ));
    }
    let exact_parent_binding = parent.revision == completed.parent_revision
        && parent.artifact_hash == completed.parent_artifact_hash
        && !parent.is_terminal();
    if !exact_parent_binding && !parent_has_evidence {
        return Err(AppError::blocked(
            "subagent parent merge exact binding 불일치",
        ));
    }
    if exact_parent_binding && !parent_has_evidence {
        let mut evidence = workflow_evidence(&parent);
        evidence.push(completed.evidence_id.clone());
        let mut merged = parent.clone();
        merged.skill_evidence = evidence.join(",");
        state::checkpoint_workflow(merged, parent.revision)?;
    }
    let installed_parent = state::load_workflow(&completed.parent_workflow_id)?;
    if installed_parent.project_id != completed.project_id
        || installed_parent.session_id != completed.session_id
        || !workflow_has_evidence(&installed_parent, &completed.evidence_id)
    {
        return Err(AppError::blocked(
            "subagent parent evidence install binding 불일치",
        ));
    }
    append_lifecycle_event(
        &ledger::validated_current_identity()?,
        completed,
        "team.subagent.result-merged",
        "subagent result merged",
    )?;
    Ok(())
}

fn workflow_evidence(parent: &state::WorkflowRecord) -> Vec<String> {
    parent
        .skill_evidence
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn workflow_has_evidence(parent: &state::WorkflowRecord, evidence_id: &str) -> bool {
    parent
        .skill_evidence
        .split(',')
        .any(|value| value == evidence_id)
}

fn recover_completed_parent_merges(parent_workflow_id: &str) -> Result<(), AppError> {
    for record in records_for_parent(parent_workflow_id)? {
        if record.status == SubagentStatus::Completed {
            merge_completed_result(&record)?;
        }
    }
    Ok(())
}

fn detail_token<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
}

fn render_worker_prompt(
    record: &SubagentRecordV1,
    task: &str,
    context: &crate::context::ContextPack,
) -> String {
    format!(
        "You are one bounded {} subagent. Return exactly one canonical compact JSON object and no surrounding text.\n\
         Required key order: schema_version, subagent_id, parent_workflow_id, role, status, summary, findings, patch_proposal, evidence_refs, validation_gaps, suggested_next_action.\n\
         Use schema_version=1, status=completed, subagent_id={}, parent_workflow_id={}, role={}.\n\
         evidence_refs must contain only declared source pointers listed below. patch_proposal must be null unless the executor render_diff capability is declared.\n\
         Never execute commands, apply patches, reveal secrets, or claim unperformed validation.\n\
         Declared tools: {}\nDeclared write ownership: {}\nTask:\n{}\n\n{}",
        record.role.as_str(),
        record.subagent_id,
        record.parent_workflow_id,
        record.role.as_str(),
        record.declared_tools.join(", "),
        display_list(&record.write_paths),
        task,
        context.prompt_section(),
    )
}

fn admit_launch(launch: ValidatedLaunch) -> Result<AdmittedLaunch, AppError> {
    let identity = ledger::validated_current_identity()?;
    let parent_workflow_id = state::active_workflow_id()?.ok_or_else(|| {
        AppError::blocked(
            "subagent admission 차단\n- 이유: active non-terminal parent workflow가 없습니다.",
        )
    })?;
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(&parent_workflow_id),
        "subagent parent admission",
    )?;
    if state::active_workflow_id()?.as_deref() != Some(parent_workflow_id.as_str()) {
        return Err(AppError::blocked(
            "subagent admission 차단\n- 이유: active parent pointer가 admission 중 변경되었습니다.",
        ));
    }
    recover_completed_parent_merges(&parent_workflow_id)?;
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(&parent_workflow_id)?;
    let parent = workflow_guard.load_current()?;
    if parent.is_terminal()
        || parent.project_id != identity.project_id
        || parent.session_id != identity.session_id
        || parent.revision == 0
        || !is_sha256(&parent.artifact_hash)
    {
        return Err(AppError::blocked(
            "subagent admission 차단\n- 이유: parent project/session/revision/hash binding이 active non-terminal 상태가 아닙니다.",
        ));
    }
    if let Some(existing) = records_for_parent(&parent.workflow_id)?
        .into_iter()
        .find(|record| !record.status.is_terminal())
    {
        if existing.status == SubagentStatus::Running {
            match lease::RecoverableLease::acquire(
                paths::project_subagent_execution_lock(&existing.subagent_id),
                "subagent execution",
            ) {
                Ok(_recovery_lease) => {
                    terminalize_locked(
                        &existing,
                        SubagentStatus::Failed,
                        "interrupted-no-replay",
                        "team.subagent.failed",
                    )?;
                }
                Err(error) if error.message.contains("subagent execution lock 차단") => {
                    append_lifecycle_event(
                        &identity,
                        &existing,
                        "team.subagent.blocked",
                        "subagent admission blocked",
                    )?;
                    return Err(AppError::blocked(format!(
                        "subagent admission 차단\n- 이유: parent당 non-terminal child는 하나만 허용합니다.\n- existing child: {}\n- existing status: {}",
                        existing.subagent_id,
                        existing.status.as_str()
                    )));
                }
                Err(error) => return Err(error),
            }
        } else {
            append_lifecycle_event(
                &identity,
                &existing,
                "team.subagent.blocked",
                "subagent admission blocked",
            )?;
            return Err(AppError::blocked(format!(
                "subagent admission 차단\n- 이유: parent당 non-terminal child는 하나만 허용합니다.\n- existing child: {}\n- existing status: {}",
                existing.subagent_id,
                existing.status.as_str()
            )));
        }
    }
    let context = crate::context::build_declared_context_pack(&launch.read_paths)?;
    let requested = create_record(SubagentRecordV1::new(
        &parent.project_id,
        &parent.session_id,
        &parent.workflow_id,
        parent.revision,
        &parent.artifact_hash,
        launch,
    )?)?;
    append_lifecycle_event(
        &identity,
        &requested,
        "team.subagent.requested",
        "subagent requested",
    )?;
    let mut admitted = requested.clone();
    admitted.transition_to(SubagentStatus::Admitted, None)?;
    let admitted = checkpoint_record(admitted, requested.revision)?;
    append_lifecycle_event(
        &identity,
        &admitted,
        "team.subagent.admitted",
        "subagent admitted",
    )?;
    Ok(AdmittedLaunch {
        record: admitted,
        context,
    })
}

fn latest_active_parent_record() -> Result<SubagentRecordV1, AppError> {
    let identity = ledger::validated_current_identity()?;
    let parent_workflow_id = state::active_workflow_id()?.ok_or_else(|| {
        AppError::blocked(
            "subagent status 차단\n- 이유: latest child를 찾을 active parent workflow가 없습니다.",
        )
    })?;
    records_for_parent(&parent_workflow_id)?
        .into_iter()
        .filter(|record| {
            record.project_id == identity.project_id && record.session_id == identity.session_id
        })
        .max_by(|left, right| {
            (left.created_at_ms, left.revision, left.subagent_id.as_str()).cmp(&(
                right.created_at_ms,
                right.revision,
                right.subagent_id.as_str(),
            ))
        })
        .ok_or_else(|| AppError::blocked("active parent에 기록된 subagent가 없습니다."))
}

fn records_for_parent(parent_workflow_id: &str) -> Result<Vec<SubagentRecordV1>, AppError> {
    let entries = match fs::read_dir(paths::project_subagents_dir()) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AppError::blocked(format!(
                "subagent state directory 읽기 실패: {err}"
            )));
        }
    };
    let mut ids = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|err| AppError::blocked(format!("subagent directory entry 오류: {err}")))?;
        if !entry
            .file_type()
            .map_err(|err| AppError::blocked(format!("subagent file type 오류: {err}")))?
            .is_file()
        {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(AppError::blocked("subagent state filename UTF-8 오류"));
        };
        let Some(subagent_id) = name.strip_suffix(".json") else {
            continue;
        };
        if !subagent_id.starts_with("subagent-") {
            continue;
        }
        ids.push(subagent_id.to_string());
        if ids.len() > MAX_SUBAGENT_RECORDS {
            return Err(AppError::blocked(format!(
                "subagent state file 상한 초과: {MAX_SUBAGENT_RECORDS}"
            )));
        }
    }
    ids.sort();
    ids.into_iter()
        .map(|subagent_id| load_record(&subagent_id))
        .collect::<Result<Vec<_>, _>>()
        .map(|records| {
            records
                .into_iter()
                .filter(|record| record.parent_workflow_id == parent_workflow_id)
                .collect()
        })
}

fn append_lifecycle_event(
    identity: &ledger::RuntimeIdentity,
    record: &SubagentRecordV1,
    event_type: &str,
    summary: &str,
) -> Result<String, AppError> {
    if record.project_id != identity.project_id || record.session_id != identity.session_id {
        return Err(AppError::blocked("subagent ledger owner binding 불일치"));
    }
    let event = ledger::new_event_for(
        identity,
        event_type,
        summary,
        &format!(
            "subagent_id={} parent_workflow_id={} parent_revision={} revision={} status={} role={} task_hash={} artifact_hash={} result_hash={}",
            record.subagent_id,
            record.parent_workflow_id,
            record.parent_revision,
            record.revision,
            record.status.as_str(),
            record.role.as_str(),
            record.task_hash,
            record.artifact_hash,
            display_value(&record.result_artifact_hash),
        ),
    );
    ledger::append_event(&event)?;
    Ok(event.event_id)
}

fn render_status_report(record: &SubagentRecordV1, action: &str) -> String {
    format!(
        "subagent status\n- action: {action}\n- subagent id: {}\n- status: {}\n- revision: {}\n- parent workflow: {}\n- parent revision: {}\n- role: {}\n- tools: {}\n- read paths: {}\n- write paths: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- backend event: {}\n- result artifact: {}\n- evidence: {}\n- failure code: {}",
        record.subagent_id,
        record.status.as_str(),
        record.revision,
        record.parent_workflow_id,
        record.parent_revision,
        record.role.as_str(),
        record.declared_tools.join(", "),
        record.read_paths.join(", "),
        display_list(&record.write_paths),
        record.requested_max_tokens,
        record.effective_max_tokens,
        display_value(&record.backend_event_id),
        display_value(&record.result_artifact_id),
        display_value(&record.evidence_id),
        display_value(&record.failure_code),
    )
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

fn display_value(value: &str) -> &str {
    if value.is_empty() {
        "없음"
    } else {
        value
    }
}

fn load_record_unlocked(subagent_id: &str) -> Result<SubagentRecordV1, AppError> {
    let path = paths::project_subagent_file(subagent_id);
    let body = fs::read_to_string(&path).map_err(|err| {
        AppError::blocked(format!(
            "subagent state 읽기 차단\n- path: {}\n- error: {err}",
            path.display()
        ))
    })?;
    let record = parse_record(&path, &body)?;
    verify_snapshot_chain(&record, &body)?;
    Ok(record)
}

fn install_snapshot(record: &SubagentRecordV1, body: &str) -> Result<(), AppError> {
    let path = paths::project_subagent_snapshot_file(&record.subagent_id, record.revision);
    if path.exists() {
        let existing = fs::read_to_string(&path)
            .map_err(|err| AppError::blocked(format!("subagent snapshot 읽기 실패: {err}")))?;
        if existing != body {
            return Err(AppError::blocked(format!(
                "subagent snapshot 충돌\n- revision: {}",
                record.revision
            )));
        }
        return Ok(());
    }
    state::atomic_replace_bytes(&path, body.as_bytes())
}

fn verify_snapshot_chain(record: &SubagentRecordV1, current_body: &str) -> Result<(), AppError> {
    if record.revision == 0 || record.revision > MAX_RECORD_REVISIONS {
        return Err(AppError::blocked("subagent revision 범위 오류"));
    }
    let mut previous_hash = "none".to_string();
    for revision in 1..=record.revision {
        let path = paths::project_subagent_snapshot_file(&record.subagent_id, revision);
        let body = fs::read_to_string(&path).map_err(|err| {
            AppError::blocked(format!(
                "subagent snapshot chain 읽기 실패\n- revision: {revision}\n- error: {err}"
            ))
        })?;
        let snapshot = parse_record(&path, &body)?;
        if snapshot.revision != revision || snapshot.previous_hash != previous_hash {
            return Err(AppError::blocked(format!(
                "subagent snapshot chain 불일치\n- revision: {revision}"
            )));
        }
        previous_hash = snapshot.artifact_hash;
        if revision == record.revision && body != current_body {
            return Err(AppError::blocked(
                "subagent current state와 latest snapshot 불일치",
            ));
        }
    }
    Ok(())
}

fn normalize_tools(role: SubagentRole, declared_tools: &[String]) -> Result<Vec<String>, AppError> {
    let mut seen = BTreeSet::new();
    for value in declared_tools {
        let tool = SubagentTool::parse(value.trim()).ok_or_else(|| {
            AppError::usage(format!("지원하지 않는 subagent tool입니다: {value}"))
        })?;
        if !role.allows(tool) {
            return Err(AppError::blocked(format!(
                "subagent role/tool policy 차단\n- role: {}\n- tool: {}",
                role.as_str(),
                tool.as_str()
            )));
        }
        if !seen.insert(tool) {
            return Err(AppError::usage(format!(
                "subagent tool은 중복 선언할 수 없습니다: {}",
                tool.as_str()
            )));
        }
    }
    if !seen.contains(&SubagentTool::ReadFile) {
        return Err(AppError::blocked(
            "v0.35 subagent는 read_file tool을 반드시 선언해야 합니다.",
        ));
    }
    Ok(seen
        .into_iter()
        .map(|tool| tool.as_str().to_string())
        .collect())
}

fn normalize_paths(kind: &str, values: &[String], required: bool) -> Result<Vec<String>, AppError> {
    if required && values.is_empty() {
        return Err(AppError::usage(format!(
            "subagent launch는 최소 하나의 --{kind} path가 필요합니다."
        )));
    }
    if values.len() > MAX_DECLARED_PATHS {
        return Err(AppError::usage(format!(
            "subagent {kind} path는 최대 {MAX_DECLARED_PATHS}개까지 허용합니다."
        )));
    }
    let mut normalized = BTreeSet::new();
    for value in values {
        let path = normalize_relative_path(value)?;
        if !normalized.insert(path.clone()) {
            return Err(AppError::usage(format!(
                "subagent {kind} path는 중복 선언할 수 없습니다: {path}"
            )));
        }
    }
    Ok(normalized.into_iter().collect())
}

pub(crate) fn normalize_relative_path(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.contains(['\\', ':']) {
        return Err(AppError::blocked(format!(
            "subagent path 정규화 차단: {value}"
        )));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(AppError::blocked(format!(
            "subagent absolute path 차단: {value}"
        )));
    }
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| AppError::blocked("subagent path는 UTF-8이어야 합니다."))?;
                if value.is_empty() {
                    return Err(AppError::blocked("subagent empty path component 차단"));
                }
                components.push(value);
            }
            _ => {
                return Err(AppError::blocked(format!(
                    "subagent path traversal 차단: {value}"
                )))
            }
        }
    }
    if components.is_empty() {
        return Err(AppError::blocked("subagent empty path 차단"));
    }
    Ok(components.join("/"))
}

fn validate_record(record: &SubagentRecordV1, installed: bool) -> Result<(), AppError> {
    validate_subagent_id(&record.subagent_id)?;
    for (label, value) in [
        ("project_id", record.project_id.as_str()),
        ("session_id", record.session_id.as_str()),
        ("parent_workflow_id", record.parent_workflow_id.as_str()),
    ] {
        if value.is_empty() || value.len() > 160 {
            return Err(AppError::blocked(format!("subagent {label} 범위 오류")));
        }
    }
    if record.parent_revision == 0 || !is_sha256(&record.parent_artifact_hash) {
        return Err(AppError::blocked("subagent parent binding 오류"));
    }
    if !is_sha256(&record.task_hash) {
        return Err(AppError::blocked("subagent task hash 오류"));
    }
    if record.timeout_ms == 0 || record.timeout_ms > backend::MAX_CHAT_TIMEOUT_MS {
        return Err(AppError::blocked("subagent timeout state 오류"));
    }
    if record.requested_max_tokens == 0
        || record.requested_max_tokens > MAX_MAX_TOKENS
        || record.effective_max_tokens == 0
        || record.effective_max_tokens > record.requested_max_tokens
    {
        return Err(AppError::blocked("subagent token state 오류"));
    }
    validate_stored_tools_and_paths(record)?;
    if record.created_at_ms == 0 {
        return Err(AppError::blocked("subagent created timestamp 누락"));
    }
    match record.status {
        SubagentStatus::Requested | SubagentStatus::Admitted => {
            if record.started_at_ms != 0
                || record.finished_at_ms != 0
                || !record.backend_event_id.is_empty()
                || has_result_binding(record)
                || !record.failure_code.is_empty()
            {
                return Err(AppError::blocked("subagent pre-run timestamp 오류"));
            }
        }
        SubagentStatus::Running => {
            if record.started_at_ms == 0
                || record.finished_at_ms != 0
                || has_result_binding(record)
                || !record.failure_code.is_empty()
            {
                return Err(AppError::blocked("subagent running timestamp 오류"));
            }
        }
        status if status.is_terminal() => {
            if record.finished_at_ms == 0
                || (record.started_at_ms != 0 && record.finished_at_ms < record.started_at_ms)
            {
                return Err(AppError::blocked("subagent terminal timestamp 오류"));
            }
            if status == SubagentStatus::Completed {
                if record.backend_event_id.is_empty()
                    || record.result_artifact_id.is_empty()
                    || !is_sha256(&record.result_artifact_hash)
                    || record.evidence_id.is_empty()
                    || !is_sha256(&record.evidence_hash)
                    || !record.failure_code.is_empty()
                {
                    return Err(AppError::blocked("subagent completed binding 오류"));
                }
            } else if record.failure_code.is_empty() || has_result_binding(record) {
                return Err(AppError::blocked(
                    "subagent terminal failure/result binding 오류",
                ));
            }
        }
        _ => unreachable!(),
    }
    if installed {
        if record.revision == 0
            || record.revision > MAX_RECORD_REVISIONS
            || (record.previous_hash != "none" && !is_sha256(&record.previous_hash))
            || !is_sha256(&record.artifact_hash)
            || state::sha256_text(&render_payload(record)) != record.artifact_hash
        {
            return Err(AppError::blocked("subagent canonical hash binding 오류"));
        }
    }
    Ok(())
}

fn has_result_binding(record: &SubagentRecordV1) -> bool {
    !record.result_artifact_id.is_empty()
        || !record.result_artifact_hash.is_empty()
        || !record.evidence_id.is_empty()
        || !record.evidence_hash.is_empty()
}

fn validate_stored_tools_and_paths(record: &SubagentRecordV1) -> Result<(), AppError> {
    let tool_inputs = record.declared_tools.clone();
    if normalize_tools(record.role, &tool_inputs)? != record.declared_tools {
        return Err(AppError::blocked("subagent canonical tool order 오류"));
    }
    let read_inputs = record.read_paths.clone();
    if normalize_paths("read", &read_inputs, true)? != record.read_paths {
        return Err(AppError::blocked("subagent canonical read path order 오류"));
    }
    let write_inputs = record.write_paths.clone();
    if normalize_paths("write", &write_inputs, false)? != record.write_paths {
        return Err(AppError::blocked(
            "subagent canonical write path order 오류",
        ));
    }
    let has_render_diff = record
        .declared_tools
        .iter()
        .any(|tool| tool == "render_diff");
    if (record.role != SubagentRole::Executor && !record.write_paths.is_empty())
        || has_render_diff != !record.write_paths.is_empty()
    {
        return Err(AppError::blocked("subagent tool/write binding 오류"));
    }
    Ok(())
}

fn immutable_binding_changed(current: &SubagentRecordV1, next: &SubagentRecordV1) -> bool {
    current.subagent_id != next.subagent_id
        || current.project_id != next.project_id
        || current.session_id != next.session_id
        || current.parent_workflow_id != next.parent_workflow_id
        || current.parent_revision != next.parent_revision
        || current.parent_artifact_hash != next.parent_artifact_hash
        || current.role != next.role
        || current.task_hash != next.task_hash
        || current.declared_tools != next.declared_tools
        || current.read_paths != next.read_paths
        || current.write_paths != next.write_paths
        || current.timeout_ms != next.timeout_ms
        || current.requested_max_tokens != next.requested_max_tokens
        || current.created_at_ms != next.created_at_ms
}

fn render_payload(record: &SubagentRecordV1) -> String {
    format!(
        "{{\"schema_version\":{SUBAGENT_SCHEMA_VERSION},\"subagent_id\":\"{}\",\"revision\":{},\"previous_hash\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"parent_revision\":{},\"parent_artifact_hash\":\"{}\",\"role\":\"{}\",\"task_hash\":\"{}\",\"declared_tools\":{},\"read_paths\":{},\"write_paths\":{},\"timeout_ms\":{},\"requested_max_tokens\":{},\"effective_max_tokens\":{},\"status\":\"{}\",\"backend_event_id\":\"{}\",\"result_artifact_id\":\"{}\",\"result_artifact_hash\":\"{}\",\"evidence_id\":\"{}\",\"evidence_hash\":\"{}\",\"failure_code\":\"{}\",\"created_at_ms\":{},\"started_at_ms\":{},\"finished_at_ms\":{}}}",
        ledger::json_string(&record.subagent_id),
        record.revision,
        ledger::json_string(&record.previous_hash),
        ledger::json_string(&record.project_id),
        ledger::json_string(&record.session_id),
        ledger::json_string(&record.parent_workflow_id),
        record.parent_revision,
        ledger::json_string(&record.parent_artifact_hash),
        ledger::json_string(record.role.as_str()),
        ledger::json_string(&record.task_hash),
        render_string_array(&record.declared_tools),
        render_string_array(&record.read_paths),
        render_string_array(&record.write_paths),
        record.timeout_ms,
        record.requested_max_tokens,
        record.effective_max_tokens,
        ledger::json_string(record.status.as_str()),
        ledger::json_string(&record.backend_event_id),
        ledger::json_string(&record.result_artifact_id),
        ledger::json_string(&record.result_artifact_hash),
        ledger::json_string(&record.evidence_id),
        ledger::json_string(&record.evidence_hash),
        ledger::json_string(&record.failure_code),
        record.created_at_ms,
        record.started_at_ms,
        record.finished_at_ms,
    )
}

fn render_record(record: &SubagentRecordV1) -> String {
    let payload = render_payload(record);
    let marker = format!(
        "\"previous_hash\":\"{}\",",
        ledger::json_string(&record.previous_hash)
    );
    let replacement = format!(
        "{marker}\"artifact_hash\":\"{}\",",
        ledger::json_string(&record.artifact_hash)
    );
    payload.replacen(&marker, &replacement, 1)
}

fn parse_record(path: &Path, body: &str) -> Result<SubagentRecordV1, AppError> {
    let context = format!("subagent canonical state: {}", path.display());
    let object = strict_json::parse_canonical_object(body, RECORD_KEYS, &context)?;
    let role_text = canonical_string(&object, "role", &context)?;
    let status_text = canonical_string(&object, "status", &context)?;
    let record = SubagentRecordV1 {
        subagent_id: canonical_string(&object, "subagent_id", &context)?,
        revision: strict_json::canonical_u64(&object, "revision", &context)?,
        previous_hash: canonical_string(&object, "previous_hash", &context)?,
        artifact_hash: canonical_string(&object, "artifact_hash", &context)?,
        project_id: canonical_string(&object, "project_id", &context)?,
        session_id: canonical_string(&object, "session_id", &context)?,
        parent_workflow_id: canonical_string(&object, "parent_workflow_id", &context)?,
        parent_revision: strict_json::canonical_u64(&object, "parent_revision", &context)?,
        parent_artifact_hash: canonical_string(&object, "parent_artifact_hash", &context)?,
        role: SubagentRole::parse(&role_text)
            .ok_or_else(|| AppError::blocked(format!("{context}: role 오류")))?,
        task_hash: canonical_string(&object, "task_hash", &context)?,
        declared_tools: canonical_string_array(&object, "declared_tools", &context)?,
        read_paths: canonical_string_array(&object, "read_paths", &context)?,
        write_paths: canonical_string_array(&object, "write_paths", &context)?,
        timeout_ms: canonical_u32(&object, "timeout_ms", &context)?,
        requested_max_tokens: canonical_u32(&object, "requested_max_tokens", &context)?,
        effective_max_tokens: canonical_u32(&object, "effective_max_tokens", &context)?,
        status: SubagentStatus::parse(&status_text)
            .ok_or_else(|| AppError::blocked(format!("{context}: status 오류")))?,
        backend_event_id: canonical_string(&object, "backend_event_id", &context)?,
        result_artifact_id: canonical_string(&object, "result_artifact_id", &context)?,
        result_artifact_hash: canonical_string(&object, "result_artifact_hash", &context)?,
        evidence_id: canonical_string(&object, "evidence_id", &context)?,
        evidence_hash: canonical_string(&object, "evidence_hash", &context)?,
        failure_code: canonical_string(&object, "failure_code", &context)?,
        created_at_ms: strict_json::canonical_u128(&object, "created_at_ms", &context)?,
        started_at_ms: strict_json::canonical_u128(&object, "started_at_ms", &context)?,
        finished_at_ms: strict_json::canonical_u128(&object, "finished_at_ms", &context)?,
    };
    if strict_json::canonical_u64(&object, "schema_version", &context)? != SUBAGENT_SCHEMA_VERSION
        || render_record(&record) != body
    {
        return Err(AppError::blocked(format!(
            "{context}: schema 또는 canonical re-render 불일치"
        )));
    }
    validate_record(&record, true)?;
    Ok(record)
}

fn canonical_string(
    object: &strict_json::CanonicalObject,
    key: &str,
    context: &str,
) -> Result<String, AppError> {
    match object.get(key) {
        Some(strict_json::CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "{context}: missing/wrong type: {key}"
        ))),
    }
}

fn canonical_string_array(
    object: &strict_json::CanonicalObject,
    key: &str,
    context: &str,
) -> Result<Vec<String>, AppError> {
    let Some(strict_json::CanonicalValue::Array(values)) = object.get(key) else {
        return Err(AppError::blocked(format!(
            "{context}: missing/wrong type: {key}"
        )));
    };
    values
        .iter()
        .map(|value| match value {
            strict_json::CanonicalValue::String(value) => Ok(value.clone()),
            _ => Err(AppError::blocked(format!(
                "{context}: array item type 오류: {key}"
            ))),
        })
        .collect()
}

fn canonical_u32(
    object: &strict_json::CanonicalObject,
    key: &str,
    context: &str,
) -> Result<u32, AppError> {
    u32::try_from(strict_json::canonical_u64(object, key, context)?)
        .map_err(|_| AppError::blocked(format!("{context}: out of range: {key}")))
}

fn render_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", ledger::json_string(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn validate_subagent_id(value: &str) -> Result<(), AppError> {
    if !value.starts_with("subagent-")
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(AppError::blocked(format!("subagent id 형식 오류: {value}")));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn now_ms() -> Result<u128, AppError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|_| AppError::runtime("subagent system clock 오류"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn launch(role: &str) -> ValidatedLaunch {
        let tools = if role == "executor" {
            strings(&["read_file", "render_diff"])
        } else {
            strings(&["read_file"])
        };
        let writes = if role == "executor" {
            strings(&["src/subagent.rs"])
        } else {
            Vec::new()
        };
        validate_launch(
            role,
            "bounded task",
            &tools,
            &strings(&["src/main.rs"]),
            &writes,
            None,
            None,
        )
        .unwrap()
    }

    fn record(role: &str) -> SubagentRecordV1 {
        SubagentRecordV1::new(
            "project-test",
            "session-test",
            "workflow-test",
            1,
            &"a".repeat(64),
            launch(role),
        )
        .unwrap()
    }

    fn initialize_parent() -> state::WorkflowRecord {
        fs::create_dir_all(paths::project_root().join("src")).unwrap();
        fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
        state::initialize().unwrap();
        state::create_workflow("subagent parent fixture").unwrap()
    }

    fn completed_result(
        record: &SubagentRecordV1,
        context: &crate::context::ContextPack,
    ) -> String {
        let evidence_ref = &context.source_pointers[0].stable_ref;
        format!(
            "{{\"schema_version\":1,\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"role\":\"{}\",\"status\":\"completed\",\"summary\":\"검증된 결과\",\"findings\":[\"선언된 파일을 확인했습니다.\"],\"patch_proposal\":null,\"evidence_refs\":[\"{}\"],\"validation_gaps\":[],\"suggested_next_action\":\"부모 작업을 계속합니다.\"}}",
            record.subagent_id,
            record.parent_workflow_id,
            record.role.as_str(),
            evidence_ref,
        )
    }

    #[test]
    fn launch_contract_enforces_role_tool_and_write_boundaries() {
        let error = validate_launch(
            "explore",
            "task",
            &strings(&["read_file", "render_diff"]),
            &strings(&["src/main.rs"]),
            &strings(&["src/main.rs"]),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.message.contains("role/tool policy"));

        let error = validate_launch(
            "executor",
            "task",
            &strings(&["read_file", "render_diff"]),
            &strings(&["src/main.rs"]),
            &[],
            None,
            None,
        )
        .unwrap_err();
        assert!(error.message.contains("함께 선언"));

        let error = validate_launch(
            "executor",
            "task",
            &strings(&["read_file", "render_diff"]),
            &strings(&["src/main.rs"]),
            &strings(&["README.md"]),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.message.contains("declared read target"));
    }

    #[test]
    fn launch_contract_enforces_exact_task_and_budget_bounds() {
        validate_launch(
            "explore",
            &"x".repeat(MAX_TASK_BYTES),
            &strings(&["read_file"]),
            &strings(&["src/main.rs"]),
            &[],
            Some(backend::MAX_CHAT_TIMEOUT_MS),
            Some(MAX_MAX_TOKENS),
        )
        .unwrap();
        for error in [
            validate_launch(
                "explore",
                &"x".repeat(MAX_TASK_BYTES + 1),
                &strings(&["read_file"]),
                &strings(&["src/main.rs"]),
                &[],
                None,
                None,
            )
            .unwrap_err(),
            validate_launch(
                "explore",
                "task",
                &strings(&["read_file"]),
                &strings(&["src/main.rs"]),
                &[],
                Some(0),
                None,
            )
            .unwrap_err(),
            validate_launch(
                "explore",
                "task",
                &strings(&["read_file"]),
                &strings(&["src/main.rs"]),
                &[],
                None,
                Some(MAX_MAX_TOKENS + 1),
            )
            .unwrap_err(),
        ] {
            assert_eq!(error.code, 2);
        }
    }

    #[test]
    fn launch_contract_rejects_traversal_duplicates_and_excess_paths() {
        for paths in [
            strings(&["../secret"]),
            strings(&["src/main.rs", "src/main.rs"]),
            strings(&["a", "b", "c", "d", "e"]),
            strings(&["C:\\secret"]),
        ] {
            let error = validate_launch(
                "explore",
                "task",
                &strings(&["read_file"]),
                &paths,
                &[],
                None,
                None,
            )
            .unwrap_err();
            assert!(matches!(error.code, 2 | 3));
        }
    }

    #[test]
    fn lifecycle_transition_matrix_is_closed() {
        let terminal = [
            SubagentStatus::Completed,
            SubagentStatus::Blocked,
            SubagentStatus::Failed,
            SubagentStatus::Cancelled,
            SubagentStatus::TimedOut,
        ];
        assert!(SubagentStatus::Requested.permits(SubagentStatus::Admitted));
        assert!(SubagentStatus::Admitted.permits(SubagentStatus::Running));
        for status in terminal {
            assert!(SubagentStatus::Running.permits(status));
            assert!(!status.permits(SubagentStatus::Requested));
        }
        assert!(!SubagentStatus::Requested.permits(SubagentStatus::Running));
        assert!(!SubagentStatus::Admitted.permits(SubagentStatus::Completed));
    }

    #[test]
    fn canonical_state_round_trips_and_preserves_hash_chain() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let requested = create_record(record("explore")).unwrap();
        let mut admitted = requested.clone();
        admitted
            .transition_to(SubagentStatus::Admitted, None)
            .unwrap();
        let admitted = checkpoint_record(admitted, requested.revision).unwrap();
        let mut running = admitted.clone();
        running
            .transition_to(SubagentStatus::Running, None)
            .unwrap();
        let running = checkpoint_record(running, admitted.revision).unwrap();
        let mut completed = running.clone();
        completed.backend_event_id = "backend-event-test".to_string();
        completed.result_artifact_id = "result-test".to_string();
        completed.result_artifact_hash = "b".repeat(64);
        completed.evidence_id = "evidence-test".to_string();
        completed.evidence_hash = "c".repeat(64);
        completed
            .transition_to(SubagentStatus::Completed, None)
            .unwrap();
        let completed = checkpoint_record(completed, running.revision).unwrap();
        assert_eq!(completed.revision, 4);
        assert_eq!(load_record(&completed.subagent_id).unwrap(), completed);
        for revision in 1..=4 {
            assert!(
                paths::project_subagent_snapshot_file(&completed.subagent_id, revision).is_file()
            );
        }
    }

    #[test]
    fn stale_revision_and_immutable_binding_changes_fail_closed() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let requested = create_record(record("explore")).unwrap();
        let mut admitted = requested.clone();
        admitted
            .transition_to(SubagentStatus::Admitted, None)
            .unwrap();
        let admitted = checkpoint_record(admitted, requested.revision).unwrap();

        let mut stale = requested.clone();
        stale
            .transition_to(SubagentStatus::Cancelled, Some("user-cancelled"))
            .unwrap();
        assert!(checkpoint_record(stale, requested.revision)
            .unwrap_err()
            .message
            .contains("stale revision"));

        let mut forged = admitted.clone();
        forged.parent_workflow_id = "workflow-other".to_string();
        forged.transition_to(SubagentStatus::Running, None).unwrap();
        assert!(checkpoint_record(forged, admitted.revision)
            .unwrap_err()
            .message
            .contains("immutable"));
    }

    #[test]
    fn tampered_current_or_snapshot_state_is_rejected() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let requested = create_record(record("explore")).unwrap();
        let current = paths::project_subagent_file(&requested.subagent_id);
        let original = fs::read_to_string(&current).unwrap();
        fs::write(&current, original.replace("requested", "admitted")).unwrap();
        assert!(load_record(&requested.subagent_id).is_err());

        fs::write(&current, &original).unwrap();
        let snapshot = paths::project_subagent_snapshot_file(&requested.subagent_id, 1);
        fs::write(&snapshot, original.replace("project-test", "project-evil")).unwrap();
        assert!(load_record(&requested.subagent_id).is_err());
    }

    #[test]
    fn conflicting_preinstalled_snapshot_blocks_checkpoint() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let pending = record("explore");
        let path = paths::project_subagent_snapshot_file(&pending.subagent_id, 1);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "forged").unwrap();
        assert!(create_record(pending)
            .unwrap_err()
            .message
            .contains("snapshot 충돌"));
    }

    #[test]
    fn admission_binds_active_parent_and_records_ordered_events() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap();
        let admitted = admitted.record;
        assert_eq!(admitted.status, SubagentStatus::Admitted);
        assert_eq!(admitted.revision, 2);
        assert_eq!(admitted.project_id, parent.project_id);
        assert_eq!(admitted.session_id, parent.session_id);
        assert_eq!(admitted.parent_workflow_id, parent.workflow_id);
        assert_eq!(admitted.parent_revision, parent.revision);
        assert_eq!(admitted.parent_artifact_hash, parent.artifact_hash);

        let lifecycle = ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type.starts_with("team.subagent."))
            .map(|event| event.event_type)
            .collect::<Vec<_>>();
        assert_eq!(
            lifecycle,
            vec![
                "team.subagent.requested".to_string(),
                "team.subagent.admitted".to_string(),
            ]
        );
    }

    #[test]
    fn admission_requires_parent_and_blocks_second_non_terminal_child() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        state::initialize().unwrap();
        assert!(admit_launch(launch("explore"))
            .unwrap_err()
            .message
            .contains("active non-terminal parent"));

        fs::create_dir_all(paths::project_root().join("src")).unwrap();
        fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
        state::create_workflow("subagent parent fixture").unwrap();
        let first = admit_launch(launch("explore")).unwrap().record;
        let error = admit_launch(launch("planner")).unwrap_err();
        assert!(error.message.contains("non-terminal child"));
        assert_eq!(
            records_for_parent(&first.parent_workflow_id).unwrap().len(),
            1
        );
    }

    #[test]
    fn admission_rejects_terminal_parent() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let mut terminal = parent.clone();
        terminal.phase = "complete".to_string();
        let terminal = state::checkpoint_workflow(terminal, parent.revision).unwrap();
        assert!(terminal.is_terminal());
        let error = admit_launch(launch("explore")).unwrap_err();
        assert!(error.message.contains("active non-terminal 상태"));
        assert!(records_for_parent(&parent.workflow_id).unwrap().is_empty());
    }

    #[test]
    fn status_defaults_to_active_parent_and_cancel_is_idempotent() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap().record;
        let status = status_report(None).unwrap();
        assert!(status.contains(&admitted.subagent_id));
        assert!(status.contains("status: admitted"));

        let cancelled_report = cancel_report(&admitted.subagent_id).unwrap();
        assert!(cancelled_report.contains("action: cancelled"));
        let cancelled = load_record(&admitted.subagent_id).unwrap();
        assert_eq!(cancelled.status, SubagentStatus::Cancelled);
        assert_eq!(cancelled.revision, 3);

        let retry = cancel_report(&admitted.subagent_id).unwrap();
        assert!(retry.contains("already-cancelled-no-op"));
        assert_eq!(load_record(&admitted.subagent_id).unwrap().revision, 3);
    }

    #[test]
    fn dispatch_completes_and_merges_evidence_once() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap();
        let response = completed_result(&admitted.record, &admitted.context);
        let completed = dispatch_admitted(admitted, "bounded task", |prompt, max, timeout| {
            assert!(prompt.contains("canonical compact JSON"));
            assert_eq!(max, DEFAULT_MAX_TOKENS);
            assert_eq!(timeout, DEFAULT_TIMEOUT_MS);
            Ok(WorkerGeneration {
                backend_event_id: "backend-event-test".to_string(),
                effective_max_tokens: 128,
                response,
            })
        })
        .unwrap();
        assert_eq!(completed.record.status, SubagentStatus::Completed);
        assert_eq!(completed.record.revision, 4);
        assert_eq!(completed.record.effective_max_tokens, 128);
        assert!(!completed.record.result_artifact_id.is_empty());
        assert!(!completed.record.evidence_id.is_empty());
        assert!(
            paths::project_subagent_result_file(&completed.record.result_artifact_id).is_file()
        );
        assert!(paths::project_evidence_dir()
            .join(format!("{}.json", completed.record.evidence_id))
            .is_file());
        let merged_parent = state::load_workflow(&parent.workflow_id).unwrap();
        assert_eq!(merged_parent.revision, parent.revision + 1);
        assert_eq!(merged_parent.skill_evidence, completed.record.evidence_id);
        assert_eq!(
            ledger::read_runtime_events()
                .unwrap()
                .into_iter()
                .filter(|event| event.event_type.starts_with("team.subagent."))
                .map(|event| event.event_type)
                .collect::<Vec<_>>(),
            vec![
                "team.subagent.requested",
                "team.subagent.admitted",
                "team.subagent.started",
                "team.subagent.completed",
                "team.subagent.result-merged",
            ]
        );
        merge_completed_result(&completed.record).unwrap();
        assert_eq!(
            ledger::read_runtime_events()
                .unwrap()
                .into_iter()
                .filter(|event| event.event_type == "team.subagent.result-merged")
                .count(),
            1
        );
    }

    #[test]
    fn admission_recovers_merge_interrupted_after_parent_checkpoint() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap();
        let (running, context) = prepare_running(&admitted).unwrap();
        let body = completed_result(&running, &context);
        let stored = crate::subagent_result::parse_and_store(&running, &context, &body).unwrap();
        crate::subagent_result::verify_stored_artifacts(&running, &stored).unwrap();

        let mut completed = running.clone();
        completed.backend_event_id = "backend-event-interrupted".to_string();
        completed.effective_max_tokens = 128;
        completed.result_artifact_id = stored.result_artifact_id;
        completed.result_artifact_hash = stored.result_artifact_hash;
        completed.evidence_id = stored.evidence_id;
        completed.evidence_hash = stored.evidence_hash;
        completed
            .transition_to(SubagentStatus::Completed, None)
            .unwrap();
        let completed = checkpoint_record(completed, running.revision).unwrap();

        let mut interrupted_parent = parent.clone();
        interrupted_parent.skill_evidence = completed.evidence_id.clone();
        let interrupted_parent =
            state::checkpoint_workflow(interrupted_parent, parent.revision).unwrap();
        assert_eq!(
            ledger::read_runtime_events()
                .unwrap()
                .into_iter()
                .filter(|event| event.event_type == "team.subagent.result-merged")
                .count(),
            0
        );

        let next = admit_launch(launch("planner")).unwrap();
        assert_eq!(next.record.parent_revision, interrupted_parent.revision);
        assert_eq!(
            state::load_workflow(&parent.workflow_id).unwrap(),
            interrupted_parent
        );
        assert_eq!(
            ledger::read_runtime_events()
                .unwrap()
                .into_iter()
                .filter(|event| event.event_type == "team.subagent.result-merged")
                .count(),
            1
        );

        merge_completed_result(&completed).unwrap();
        assert_eq!(
            ledger::read_runtime_events()
                .unwrap()
                .into_iter()
                .filter(|event| event.event_type == "team.subagent.result-merged")
                .count(),
            1
        );
    }

    #[test]
    fn dispatch_blocks_invalid_result_without_parent_merge() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap();
        let subagent_id = admitted.record.subagent_id.clone();
        let error = dispatch_admitted(admitted, "bounded task", |_, _, _| {
            Ok(WorkerGeneration {
                backend_event_id: "backend-event-invalid".to_string(),
                effective_max_tokens: 128,
                response: "{}".to_string(),
            })
        })
        .unwrap_err();
        assert!(error.message.contains("result 검증 차단"));
        let blocked = load_record(&subagent_id).unwrap();
        assert_eq!(blocked.status, SubagentStatus::Blocked);
        assert_eq!(blocked.failure_code, "invalid-result");
        assert_eq!(state::load_workflow(&parent.workflow_id).unwrap(), parent);
    }

    #[test]
    fn dispatch_timeout_discards_partial_output_and_records_timed_out() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap();
        let subagent_id = admitted.record.subagent_id.clone();
        let error = dispatch_admitted(admitted, "bounded task", |_, _, _| {
            Err(AppError::runtime(
                "backend chat 중단: 제한 시간 초과로 취소됨",
            ))
        })
        .unwrap_err();
        assert!(error.message.contains("partial output: discarded"));
        let timed_out = load_record(&subagent_id).unwrap();
        assert_eq!(timed_out.status, SubagentStatus::TimedOut);
        assert_eq!(timed_out.failure_code, "backend-timeout");
        assert!(timed_out.result_artifact_id.is_empty());
    }

    #[test]
    fn dispatch_resource_denial_records_blocked_without_result_or_parent_merge() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap();
        let subagent_id = admitted.record.subagent_id.clone();
        let error = dispatch_admitted(admitted, "bounded task", |_, _, _| {
            Err(AppError::blocked(
                "backend chat resource governor 차단: critical pressure",
            ))
        })
        .unwrap_err();
        assert!(error.message.contains("resource governor"));
        let blocked = load_record(&subagent_id).unwrap();
        assert_eq!(blocked.status, SubagentStatus::Blocked);
        assert_eq!(blocked.failure_code, "backend-blocked");
        assert!(blocked.backend_event_id.is_empty());
        assert!(blocked.result_artifact_id.is_empty());
        assert!(blocked.evidence_id.is_empty());
        assert_eq!(state::load_workflow(&parent.workflow_id).unwrap(), parent);
        assert!(ledger::read_runtime_events()
            .unwrap()
            .iter()
            .any(|event| event.event_type == "team.subagent.blocked"));
    }

    #[test]
    fn manual_cancel_wins_before_backend_completion_merge() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap();
        let subagent_id = admitted.record.subagent_id.clone();
        let response = completed_result(&admitted.record, &admitted.context);
        let error = dispatch_admitted(admitted, "bounded task", |_, _, _| {
            let report = cancel_report(&subagent_id).unwrap();
            assert!(report.contains("action: cancelled"));
            Ok(WorkerGeneration {
                backend_event_id: "backend-event-after-cancel".to_string(),
                effective_max_tokens: 128,
                response,
            })
        })
        .unwrap_err();
        assert!(error.message.contains("cancellation이 먼저"));
        let cancelled = load_record(&subagent_id).unwrap();
        assert_eq!(cancelled.status, SubagentStatus::Cancelled);
        assert!(cancelled.result_artifact_id.is_empty());
        assert_eq!(state::load_workflow(&parent.workflow_id).unwrap(), parent);
    }

    #[test]
    fn stale_parent_or_context_blocks_completion_without_merge() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap();
        let subagent_id = admitted.record.subagent_id.clone();
        let response = completed_result(&admitted.record, &admitted.context);
        let mut changed_parent = parent.clone();
        changed_parent.result_summary = "parent changed".to_string();
        let error = dispatch_admitted(admitted, "bounded task", |_, _, _| {
            state::checkpoint_workflow(changed_parent, parent.revision).unwrap();
            Ok(WorkerGeneration {
                backend_event_id: "backend-event-stale-parent".to_string(),
                effective_max_tokens: 128,
                response,
            })
        })
        .unwrap_err();
        assert!(error.message.contains("stale parent"));
        assert_eq!(
            load_record(&subagent_id).unwrap().failure_code,
            "stale-parent"
        );

        let current_parent = state::load_workflow(&parent.workflow_id).unwrap();
        let admitted = admit_launch(launch("explore")).unwrap();
        let subagent_id = admitted.record.subagent_id.clone();
        let response = completed_result(&admitted.record, &admitted.context);
        let error = dispatch_admitted(admitted, "bounded task", |_, _, _| {
            fs::write(
                paths::project_root().join("src/main.rs"),
                "fn main() { changed(); }\n",
            )
            .unwrap();
            Ok(WorkerGeneration {
                backend_event_id: "backend-event-stale-context".to_string(),
                effective_max_tokens: 128,
                response,
            })
        })
        .unwrap_err();
        assert!(error.message.contains("source binding"));
        assert_eq!(
            load_record(&subagent_id).unwrap().failure_code,
            "stale-context"
        );
        assert_eq!(
            state::load_workflow(&parent.workflow_id).unwrap(),
            current_parent
        );
    }

    #[test]
    fn stale_running_child_recovers_as_failed_without_backend_replay() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        initialize_parent();
        let admitted = admit_launch(launch("explore")).unwrap().record;
        let mut running = admitted.clone();
        running
            .transition_to(SubagentStatus::Running, None)
            .unwrap();
        let running = checkpoint_record(running, admitted.revision).unwrap();

        let replacement = admit_launch(launch("planner")).unwrap().record;
        let recovered = load_record(&running.subagent_id).unwrap();
        assert_eq!(recovered.status, SubagentStatus::Failed);
        assert_eq!(recovered.failure_code, "interrupted-no-replay");
        assert_eq!(replacement.status, SubagentStatus::Admitted);
        assert_ne!(replacement.subagent_id, recovered.subagent_id);
    }
}
