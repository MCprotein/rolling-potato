//! Subagent role, tool, launch, and lifecycle policy.

use crate::foundation::error::AppError;
use crate::foundation::integrity;
use crate::foundation::serialization as strict_json;
pub(crate) use crate::runtime_core::inference::backend::MAX_CHAT_TIMEOUT_MS;
use std::collections::BTreeSet;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SUBAGENT_SCHEMA_VERSION: u64 = 1;
pub(crate) const MAX_RECORD_REVISIONS: u64 = 4;
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

pub const DEFAULT_TIMEOUT_MS: u32 = 30_000;
pub const DEFAULT_MAX_TOKENS: u32 = 256;
pub const MAX_MAX_TOKENS: u32 = 1_024;
pub const MAX_TASK_BYTES: usize = 4_096;
pub const MAX_DECLARED_PATHS: usize = 4;

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

    pub(crate) fn permits(self, next: Self) -> bool {
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
            subagent_id: format!("subagent-{}", &integrity::sha256_text(&nonce)[..20]),
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
    if timeout_ms == 0 || timeout_ms > MAX_CHAT_TIMEOUT_MS {
        return Err(AppError::usage(format!(
            "subagent timeout은 1..={} ms 범위여야 합니다.",
            MAX_CHAT_TIMEOUT_MS
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
        task_hash: integrity::sha256_text(task),
        declared_tools: tools,
        read_paths,
        write_paths,
        timeout_ms,
        requested_max_tokens,
    })
}

pub(crate) fn normalize_tools(
    role: SubagentRole,
    declared_tools: &[String],
) -> Result<Vec<String>, AppError> {
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

pub(crate) fn normalize_paths(
    kind: &str,
    values: &[String],
    required: bool,
) -> Result<Vec<String>, AppError> {
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

pub(crate) fn validate_record(record: &SubagentRecordV1, installed: bool) -> Result<(), AppError> {
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
    if record.timeout_ms == 0 || record.timeout_ms > MAX_CHAT_TIMEOUT_MS {
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
    if installed
        && (record.revision == 0
            || record.revision > MAX_RECORD_REVISIONS
            || (record.previous_hash != "none" && !is_sha256(&record.previous_hash))
            || !is_sha256(&record.artifact_hash)
            || integrity::sha256_text(&render_payload(record)) != record.artifact_hash)
    {
        return Err(AppError::blocked("subagent canonical hash binding 오류"));
    }
    Ok(())
}

pub(crate) fn immutable_binding_changed(
    current: &SubagentRecordV1,
    next: &SubagentRecordV1,
) -> bool {
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

pub(crate) fn render_payload(record: &SubagentRecordV1) -> String {
    format!(
        "{{\"schema_version\":{SUBAGENT_SCHEMA_VERSION},\"subagent_id\":\"{}\",\"revision\":{},\"previous_hash\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"parent_revision\":{},\"parent_artifact_hash\":\"{}\",\"role\":\"{}\",\"task_hash\":\"{}\",\"declared_tools\":{},\"read_paths\":{},\"write_paths\":{},\"timeout_ms\":{},\"requested_max_tokens\":{},\"effective_max_tokens\":{},\"status\":\"{}\",\"backend_event_id\":\"{}\",\"result_artifact_id\":\"{}\",\"result_artifact_hash\":\"{}\",\"evidence_id\":\"{}\",\"evidence_hash\":\"{}\",\"failure_code\":\"{}\",\"created_at_ms\":{},\"started_at_ms\":{},\"finished_at_ms\":{}}}",
        escape(&record.subagent_id),
        record.revision,
        escape(&record.previous_hash),
        escape(&record.project_id),
        escape(&record.session_id),
        escape(&record.parent_workflow_id),
        record.parent_revision,
        escape(&record.parent_artifact_hash),
        escape(record.role.as_str()),
        escape(&record.task_hash),
        render_string_array(&record.declared_tools),
        render_string_array(&record.read_paths),
        render_string_array(&record.write_paths),
        record.timeout_ms,
        record.requested_max_tokens,
        record.effective_max_tokens,
        escape(record.status.as_str()),
        escape(&record.backend_event_id),
        escape(&record.result_artifact_id),
        escape(&record.result_artifact_hash),
        escape(&record.evidence_id),
        escape(&record.evidence_hash),
        escape(&record.failure_code),
        record.created_at_ms,
        record.started_at_ms,
        record.finished_at_ms,
    )
}

pub(crate) fn render_record(record: &SubagentRecordV1) -> String {
    let payload = render_payload(record);
    let marker = format!("\"previous_hash\":\"{}\",", escape(&record.previous_hash));
    let replacement = format!(
        "{marker}\"artifact_hash\":\"{}\",",
        escape(&record.artifact_hash)
    );
    payload.replacen(&marker, &replacement, 1)
}

pub(crate) fn parse_record(context: &str, body: &str) -> Result<SubagentRecordV1, AppError> {
    let object = strict_json::parse_canonical_object(body, RECORD_KEYS, context)?;
    let role_text = canonical_string(&object, "role", context)?;
    let status_text = canonical_string(&object, "status", context)?;
    let record = SubagentRecordV1 {
        subagent_id: canonical_string(&object, "subagent_id", context)?,
        revision: strict_json::canonical_u64(&object, "revision", context)?,
        previous_hash: canonical_string(&object, "previous_hash", context)?,
        artifact_hash: canonical_string(&object, "artifact_hash", context)?,
        project_id: canonical_string(&object, "project_id", context)?,
        session_id: canonical_string(&object, "session_id", context)?,
        parent_workflow_id: canonical_string(&object, "parent_workflow_id", context)?,
        parent_revision: strict_json::canonical_u64(&object, "parent_revision", context)?,
        parent_artifact_hash: canonical_string(&object, "parent_artifact_hash", context)?,
        role: SubagentRole::parse(&role_text)
            .ok_or_else(|| AppError::blocked(format!("{context}: role 오류")))?,
        task_hash: canonical_string(&object, "task_hash", context)?,
        declared_tools: canonical_string_array(&object, "declared_tools", context)?,
        read_paths: canonical_string_array(&object, "read_paths", context)?,
        write_paths: canonical_string_array(&object, "write_paths", context)?,
        timeout_ms: canonical_u32(&object, "timeout_ms", context)?,
        requested_max_tokens: canonical_u32(&object, "requested_max_tokens", context)?,
        effective_max_tokens: canonical_u32(&object, "effective_max_tokens", context)?,
        status: SubagentStatus::parse(&status_text)
            .ok_or_else(|| AppError::blocked(format!("{context}: status 오류")))?,
        backend_event_id: canonical_string(&object, "backend_event_id", context)?,
        result_artifact_id: canonical_string(&object, "result_artifact_id", context)?,
        result_artifact_hash: canonical_string(&object, "result_artifact_hash", context)?,
        evidence_id: canonical_string(&object, "evidence_id", context)?,
        evidence_hash: canonical_string(&object, "evidence_hash", context)?,
        failure_code: canonical_string(&object, "failure_code", context)?,
        created_at_ms: strict_json::canonical_u128(&object, "created_at_ms", context)?,
        started_at_ms: strict_json::canonical_u128(&object, "started_at_ms", context)?,
        finished_at_ms: strict_json::canonical_u128(&object, "finished_at_ms", context)?,
    };
    if strict_json::canonical_u64(&object, "schema_version", context)? != SUBAGENT_SCHEMA_VERSION
        || render_record(&record) != body
    {
        return Err(AppError::blocked(format!(
            "{context}: schema 또는 canonical re-render 불일치"
        )));
    }
    validate_record(&record, true)?;
    Ok(record)
}

pub(crate) fn validate_subagent_id(value: &str) -> Result<(), AppError> {
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

pub(crate) fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
            .map(|value| format!("\"{}\"", escape(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn escape(value: &str) -> String {
    strict_json::escape_string_content(value)
}

fn now_ms() -> Result<u128, AppError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|_| AppError::runtime("subagent system clock 오류"))
}
