use crate::app::inference_adapter::backend;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent as subagent_policy;
pub(crate) use crate::runtime_core::collaboration::subagent::*;
use crate::{adapters::filesystem::layout as paths, adapters::filesystem::lease, ledger, state};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_SUBAGENT_RECORDS: usize = 256;
static SUBAGENT_ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(crate) struct AdmittedLaunch {
    record: SubagentRecordV1,
    context: crate::context::ContextPack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkerGeneration {
    pub backend_event_id: String,
    pub effective_max_tokens: u32,
    pub response: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TeamMemberLaunch {
    pub lane: u32,
    pub member_id: String,
    pub role: String,
    pub task: String,
    pub declared_tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub max_tokens: u32,
}

#[derive(Debug)]
pub(crate) struct AdmittedTeamMember {
    pub lane: u32,
    pub member_id: String,
    task: String,
    admitted: AdmittedLaunch,
}

pub(crate) struct PreparedTeamMember {
    pub lane: u32,
    pub member_id: String,
    prepared: PreparedLaunch,
}

impl PreparedTeamMember {
    pub fn subagent_id(&self) -> &str {
        &self.prepared.running.subagent_id
    }
}

impl AdmittedTeamMember {
    pub fn subagent_id(&self) -> &str {
        &self.admitted.record.subagent_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedTeamMember {
    pub lane: u32,
    pub member_id: String,
    pub record: SubagentRecordV1,
    pub summary: String,
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
        subagent_policy::create_record_at(
            NewRecordBinding {
                subagent_id: format!("subagent-{}", &state::sha256_text(&nonce)[..20]),
                project_id,
                session_id,
                parent_workflow_id,
                parent_revision,
                parent_artifact_hash,
                created_at_ms,
            },
            launch,
        )
    }

    pub fn transition_to(
        &mut self,
        next: SubagentStatus,
        failure_code: Option<&str>,
    ) -> Result<(), AppError> {
        self.transition_to_at(next, failure_code, now_ms()?)
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
    let record = parse_record(
        &format!("subagent canonical state: {}", path.display()),
        &before,
    )?;
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
    let completed = dispatch_admitted(admitted, task, true, |prompt, max_tokens, timeout_ms| {
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

pub(crate) fn admit_team_members(
    parent_workflow_id: &str,
    parent_revision: u64,
    parent_artifact_hash: &str,
    members: Vec<TeamMemberLaunch>,
) -> Result<Vec<AdmittedTeamMember>, AppError> {
    if members.is_empty() {
        return Err(AppError::blocked("team execution member가 없습니다."));
    }
    let mut prepared = Vec::with_capacity(members.len());
    for member in members {
        let launch = validate_launch(
            &member.role,
            &member.task,
            &member.declared_tools,
            &member.read_paths,
            &member.write_paths,
            Some(member.timeout_ms),
            Some(member.max_tokens),
        )?;
        let context = crate::context::build_declared_context_pack(&launch.read_paths)?;
        prepared.push((member, launch, context));
    }

    recover_completed_parent_merges(parent_workflow_id)?;
    let identity = ledger::validated_current_identity()?;
    let _parent_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_parent_lock(parent_workflow_id),
        "team member admission",
    )?;
    if state::active_workflow_id()?.as_deref() != Some(parent_workflow_id) {
        return Err(AppError::blocked(
            "team member admission 차단: active parent pointer 변경",
        ));
    }
    let workflow_guard = state::WorkflowCheckpointGuard::acquire(parent_workflow_id)?;
    let parent = workflow_guard.load_current()?;
    if parent.is_terminal()
        || parent.project_id != identity.project_id
        || parent.session_id != identity.session_id
        || parent.revision != parent_revision
        || parent.artifact_hash != parent_artifact_hash
    {
        return Err(AppError::blocked(
            "team member admission 차단: exact parent binding 변경",
        ));
    }
    if let Some(existing) = records_for_parent(parent_workflow_id)?
        .into_iter()
        .find(|record| !record.status.is_terminal())
    {
        return Err(AppError::blocked(format!(
            "team member admission 차단: 기존 non-terminal child가 있습니다.\n- subagent id: {}\n- status: {}",
            existing.subagent_id,
            existing.status.as_str()
        )));
    }

    let mut admitted_members = Vec::with_capacity(prepared.len());
    for (member, launch, context) in prepared {
        let result = (|| {
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
                "team member requested",
            )?;
            let mut admitted = requested.clone();
            admitted.transition_to(SubagentStatus::Admitted, None)?;
            let admitted = checkpoint_record(admitted, requested.revision)?;
            append_lifecycle_event(
                &identity,
                &admitted,
                "team.subagent.admitted",
                "team member admitted",
            )?;
            Ok(AdmittedTeamMember {
                lane: member.lane,
                member_id: member.member_id,
                task: member.task,
                admitted: AdmittedLaunch {
                    record: admitted,
                    context,
                },
            })
        })();
        match result {
            Ok(admitted) => admitted_members.push(admitted),
            Err(error) => {
                for admitted in &admitted_members {
                    let current = load_record(&admitted.admitted.record.subagent_id)?;
                    if !current.status.is_terminal() {
                        terminalize_locked(
                            &current,
                            SubagentStatus::Cancelled,
                            "team-admission-rollback",
                            "team.subagent.cancelled",
                        )?;
                    }
                }
                return Err(error);
            }
        }
    }
    Ok(admitted_members)
}

pub(crate) fn resume_admitted_team_member(
    member: TeamMemberLaunch,
    subagent_id: &str,
) -> Result<AdmittedTeamMember, AppError> {
    let launch = validate_launch(
        &member.role,
        &member.task,
        &member.declared_tools,
        &member.read_paths,
        &member.write_paths,
        Some(member.timeout_ms),
        Some(member.max_tokens),
    )?;
    let record = load_record(subagent_id)?;
    if record.status != SubagentStatus::Admitted
        || record.role != launch.role
        || record.task_hash != launch.task_hash
        || record.declared_tools != launch.declared_tools
        || record.read_paths != launch.read_paths
        || record.write_paths != launch.write_paths
        || record.timeout_ms != launch.timeout_ms
        || record.requested_max_tokens != launch.requested_max_tokens
    {
        return Err(AppError::blocked(
            "team admitted recovery immutable launch binding 불일치",
        ));
    }
    let context = crate::context::build_declared_context_pack(&record.read_paths)?;
    Ok(AdmittedTeamMember {
        lane: member.lane,
        member_id: member.member_id,
        task: member.task,
        admitted: AdmittedLaunch { record, context },
    })
}

pub(crate) fn terminalize_interrupted_team_members(
    subagent_ids: &[String],
) -> Result<Vec<SubagentRecordV1>, AppError> {
    let mut execution_leases = Vec::new();
    for subagent_id in subagent_ids {
        let current = load_record(subagent_id)?;
        if !current.status.is_terminal() {
            execution_leases.push(lease::RecoverableLease::acquire(
                paths::project_subagent_execution_lock(subagent_id),
                "subagent interrupted recovery",
            )?);
        }
    }
    let Some(first_id) = subagent_ids.first() else {
        return Ok(Vec::new());
    };
    let first = load_record(first_id)?;
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&first.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
    )?;
    let mut recovered = Vec::with_capacity(subagent_ids.len());
    for subagent_id in subagent_ids {
        let current = load_record(subagent_id)?;
        let terminal = match current.status {
            SubagentStatus::Requested | SubagentStatus::Admitted => terminalize_locked(
                &current,
                SubagentStatus::Cancelled,
                "team-interrupted-before-send",
                "team.subagent.cancelled",
            )?,
            SubagentStatus::Running => terminalize_locked(
                &current,
                SubagentStatus::Failed,
                "interrupted-no-replay",
                "team.subagent.failed",
            )?,
            _ => current,
        };
        recovered.push(terminal);
    }
    drop(execution_leases);
    Ok(recovered)
}

pub(crate) fn execute_admitted_team_member_with(
    member: AdmittedTeamMember,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedTeamMember, AppError> {
    let completed = dispatch_admitted(member.admitted, &member.task, false, runner)?;
    Ok(CompletedTeamMember {
        lane: member.lane,
        member_id: member.member_id,
        record: completed.record,
        summary: completed.summary,
    })
}

pub(crate) fn prepare_team_members(
    members: Vec<AdmittedTeamMember>,
) -> Result<Vec<PreparedTeamMember>, AppError> {
    let subagent_ids = members
        .iter()
        .map(|member| member.subagent_id().to_string())
        .collect::<Vec<_>>();
    let mut prepared_members = Vec::with_capacity(members.len());
    for member in members {
        match prepare_admitted_launch(member.admitted, member.task) {
            Ok(prepared) => prepared_members.push(PreparedTeamMember {
                lane: member.lane,
                member_id: member.member_id,
                prepared,
            }),
            Err(error) => {
                rollback_team_preparation(&subagent_ids)?;
                return Err(error);
            }
        }
    }
    Ok(prepared_members)
}

pub(crate) fn execute_prepared_team_member_with(
    member: PreparedTeamMember,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedTeamMember, AppError> {
    let completed = execute_prepared_launch(member.prepared, false, runner)?;
    Ok(CompletedTeamMember {
        lane: member.lane,
        member_id: member.member_id,
        record: completed.record,
        summary: completed.summary,
    })
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

struct PreparedLaunch {
    _execution_lease: lease::RecoverableLease,
    running: SubagentRecordV1,
    context: crate::context::ContextPack,
    task: String,
}

fn dispatch_admitted(
    admitted: AdmittedLaunch,
    task: &str,
    merge_parent: bool,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedLaunch, AppError> {
    let prepared = prepare_admitted_launch(admitted, task.to_string())?;
    execute_prepared_launch(prepared, merge_parent, runner)
}

fn prepare_admitted_launch(
    admitted: AdmittedLaunch,
    task: String,
) -> Result<PreparedLaunch, AppError> {
    let execution_lease = lease::RecoverableLease::acquire(
        paths::project_subagent_execution_lock(&admitted.record.subagent_id),
        "subagent execution",
    )?;
    let (running, context) = prepare_running(&admitted)?;
    Ok(PreparedLaunch {
        _execution_lease: execution_lease,
        running,
        context,
        task,
    })
}

fn execute_prepared_launch(
    prepared: PreparedLaunch,
    merge_parent: bool,
    runner: impl FnOnce(&str, u32, u32) -> Result<WorkerGeneration, AppError>,
) -> Result<CompletedLaunch, AppError> {
    let PreparedLaunch {
        _execution_lease,
        running,
        context,
        task,
    } = prepared;
    let prompt = render_worker_prompt(&running, &task, &context);
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
    complete_generation(running, context, generation, merge_parent)
}

fn rollback_team_preparation(subagent_ids: &[String]) -> Result<(), AppError> {
    let Some(first_id) = subagent_ids.first() else {
        return Ok(());
    };
    let first = load_record(first_id)?;
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&first.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
    )?;
    for subagent_id in subagent_ids {
        let current = load_record(subagent_id)?;
        if !current.status.is_terminal() {
            terminalize_locked(
                &current,
                SubagentStatus::Cancelled,
                "team-prepare-rollback",
                "team.subagent.cancelled",
            )?;
        }
    }
    Ok(())
}

fn prepare_running(
    admitted: &AdmittedLaunch,
) -> Result<(SubagentRecordV1, crate::context::ContextPack), AppError> {
    let record = &admitted.record;
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&record.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
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
    merge_parent: bool,
) -> Result<CompletedLaunch, AppError> {
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&running.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
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
    if merge_parent {
        merge_completed_result(&completed)?;
    }
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
    let _parent_lease = lease::RecoverableLease::acquire_with_wait(
        paths::project_subagent_parent_lock(&running.parent_workflow_id),
        "subagent parent admission",
        Duration::from_secs(5),
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

pub(crate) fn records_for_parent(
    parent_workflow_id: &str,
) -> Result<Vec<SubagentRecordV1>, AppError> {
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
    let record = parse_record(
        &format!("subagent canonical state: {}", path.display()),
        &body,
    )?;
    verify_snapshot_chain(&record, &body)?;
    Ok(record)
}

fn now_ms() -> Result<u128, AppError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|_| AppError::runtime("subagent system clock 오류"))
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
        let snapshot = parse_record(
            &format!("subagent canonical state: {}", path.display()),
            &body,
        )?;
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
            Some(MAX_CHAT_TIMEOUT_MS),
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
        let completed =
            dispatch_admitted(admitted, "bounded task", true, |prompt, max, timeout| {
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
        let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
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
        let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
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
        let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
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
        let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
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
        let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
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
        let error = dispatch_admitted(admitted, "bounded task", true, |_, _, _| {
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
