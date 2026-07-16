use crate::app::AppError;
use crate::{backend, lease, ledger, paths, state, strict_json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;
pub const DEFAULT_MAX_TOKENS: u32 = 256;
pub const MAX_MAX_TOKENS: u32 = 1_024;
pub const MAX_TASK_BYTES: usize = 4_096;
pub const MAX_DECLARED_PATHS: usize = 4;
pub const MAX_RESULT_BYTES: usize = 65_536;

const SUBAGENT_SCHEMA_VERSION: u64 = 1;
const MAX_RECORD_REVISIONS: u64 = 4;
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
            "{project_id}\n{session_id}\n{parent_workflow_id}\n{}\n{created_at_ms}",
            launch.task_hash
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

fn normalize_relative_path(value: &str) -> Result<String, AppError> {
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
            if record.started_at_ms != 0 || record.finished_at_ms != 0 {
                return Err(AppError::blocked("subagent pre-run timestamp 오류"));
            }
        }
        SubagentStatus::Running => {
            if record.started_at_ms == 0 || record.finished_at_ms != 0 {
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
                if record.result_artifact_id.is_empty()
                    || !is_sha256(&record.result_artifact_hash)
                    || record.evidence_id.is_empty()
                    || !is_sha256(&record.evidence_hash)
                    || !record.failure_code.is_empty()
                {
                    return Err(AppError::blocked("subagent completed binding 오류"));
                }
            } else if record.failure_code.is_empty() {
                return Err(AppError::blocked("subagent terminal failure code 누락"));
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
}
