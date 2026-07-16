use crate::app::AppError;
use crate::{lease, ledger, observability, paths, state, strict_json, subagent};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const TEAM_SCHEMA_VERSION: u64 = 1;
const MAX_TEAM_LANES: usize = 4;
const MAX_TEAM_ID_BYTES: usize = 64;
const MAX_MEMBER_ID_BYTES: usize = 64;
const MAX_MANIFEST_BYTES: u64 = 65_536;
const MAX_STATE_BYTES: u64 = 65_536;
const MAX_STATE_REVISIONS: u64 = 64;
const MAX_TEAM_RECORDS: usize = 256;

const MANIFEST_KEYS: &[&str] = &[
    "schema_version",
    "team_id",
    "parent_workflow_id",
    "members",
    "write_policy",
    "merge_policy",
    "stop_gate",
];
const MEMBER_KEYS: &[&str] = &[
    "lane",
    "id",
    "role",
    "task",
    "tools",
    "read_paths",
    "write_paths",
    "timeout_ms",
    "max_tokens",
];
const STATE_KEYS: &[&str] = &[
    "schema_version",
    "team_id",
    "revision",
    "previous_hash",
    "artifact_hash",
    "manifest_hash",
    "project_id",
    "session_id",
    "parent_workflow_id",
    "parent_revision",
    "parent_artifact_hash",
    "stage",
    "status",
    "requested_lanes",
    "admitted_lanes",
    "execution_mode",
    "member_count",
    "created_at_ms",
    "updated_at_ms",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMemberV1 {
    pub lane: u32,
    pub member_id: String,
    pub role: String,
    pub task: String,
    pub task_hash: String,
    pub tools: Vec<String>,
    pub read_paths: Vec<String>,
    pub write_paths: Vec<String>,
    pub timeout_ms: u32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamManifestV1 {
    pub team_id: String,
    pub parent_workflow_id: String,
    pub members: Vec<TeamMemberV1>,
    pub write_policy: String,
    pub merge_policy: String,
    pub stop_gate: String,
    pub artifact_hash: String,
    canonical_body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamStage {
    Plan,
    Dispatch,
    Execute,
    Review,
    Verify,
    Merge,
    Report,
    Complete,
    Failed,
    Cancelled,
}

impl TeamStage {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "team-plan" => Some(Self::Plan),
            "team-dispatch" => Some(Self::Dispatch),
            "team-exec" => Some(Self::Execute),
            "team-review" => Some(Self::Review),
            "team-verify" => Some(Self::Verify),
            "team-merge" => Some(Self::Merge),
            "team-report" => Some(Self::Report),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "team-plan",
            Self::Dispatch => "team-dispatch",
            Self::Execute => "team-exec",
            Self::Review => "team-review",
            Self::Verify => "team-verify",
            Self::Merge => "team-merge",
            Self::Report => "team-report",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled)
    }

    fn permits(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Plan, Self::Dispatch)
                | (Self::Dispatch, Self::Execute)
                | (Self::Execute, Self::Review)
                | (Self::Review, Self::Verify)
                | (Self::Verify, Self::Merge)
                | (Self::Merge, Self::Report)
                | (Self::Report, Self::Complete)
        ) || (!self.is_terminal() && matches!(next, Self::Failed | Self::Cancelled))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamStateV1 {
    pub team_id: String,
    pub revision: u64,
    pub previous_hash: String,
    pub artifact_hash: String,
    pub manifest_hash: String,
    pub project_id: String,
    pub session_id: String,
    pub parent_workflow_id: String,
    pub parent_revision: u64,
    pub parent_artifact_hash: String,
    pub stage: TeamStage,
    pub status: String,
    pub requested_lanes: u32,
    pub admitted_lanes: u32,
    pub execution_mode: String,
    pub member_count: u32,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

impl TeamStateV1 {
    fn new(
        identity: &ledger::RuntimeIdentity,
        parent: &state::WorkflowRecord,
        manifest: &TeamManifestV1,
    ) -> Result<Self, AppError> {
        let timestamp = now_ms()?;
        Ok(Self {
            team_id: manifest.team_id.clone(),
            revision: 0,
            previous_hash: String::new(),
            artifact_hash: String::new(),
            manifest_hash: manifest.artifact_hash.clone(),
            project_id: identity.project_id.clone(),
            session_id: identity.session_id.clone(),
            parent_workflow_id: parent.workflow_id.clone(),
            parent_revision: parent.revision,
            parent_artifact_hash: parent.artifact_hash.clone(),
            stage: TeamStage::Plan,
            status: "active".to_string(),
            requested_lanes: manifest.members.len() as u32,
            admitted_lanes: 0,
            execution_mode: "pending".to_string(),
            member_count: manifest.members.len() as u32,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        })
    }

    pub fn transition_to(
        &mut self,
        next: TeamStage,
        admitted_lanes: Option<u32>,
        execution_mode: Option<&str>,
    ) -> Result<(), AppError> {
        if !self.stage.permits(next) {
            return Err(AppError::blocked(format!(
                "team stage 전이 차단\n- current: {}\n- next: {}",
                self.stage.as_str(),
                next.as_str()
            )));
        }
        if next == TeamStage::Dispatch {
            let admitted_lanes = admitted_lanes.ok_or_else(|| {
                AppError::blocked("team dispatch stage에는 admitted lane 수가 필요합니다.")
            })?;
            if admitted_lanes == 0 || admitted_lanes > self.requested_lanes {
                return Err(AppError::blocked(
                    "team dispatch admitted lane binding이 요청 범위를 벗어났습니다.",
                ));
            }
            let execution_mode = execution_mode.unwrap_or("");
            if !matches!(execution_mode, "parallel" | "sequential") {
                return Err(AppError::blocked(
                    "team dispatch execution mode는 parallel 또는 sequential이어야 합니다.",
                ));
            }
            self.admitted_lanes = admitted_lanes;
            self.execution_mode = execution_mode.to_string();
        } else if admitted_lanes.is_some() || execution_mode.is_some() {
            return Err(AppError::blocked(
                "team dispatch 외 stage에서 admission binding을 변경할 수 없습니다.",
            ));
        }
        self.stage = next;
        self.status = match next {
            TeamStage::Complete => "completed",
            TeamStage::Failed => "failed",
            TeamStage::Cancelled => "cancelled",
            _ => "active",
        }
        .to_string();
        self.updated_at_ms = now_ms()?;
        Ok(())
    }
}

pub fn plan_report(manifest_path: &str) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let parent_workflow_id = state::active_workflow_id()?.ok_or_else(|| {
        AppError::blocked("team plan 차단\n- 이유: active non-terminal parent workflow가 없습니다.")
    })?;
    let parent = state::load_workflow(&parent_workflow_id)?;
    if parent.is_terminal()
        || parent.project_id != identity.project_id
        || parent.session_id != identity.session_id
        || parent.revision == 0
        || !is_sha256(&parent.artifact_hash)
    {
        return Err(AppError::blocked(
            "team plan 차단\n- 이유: active parent binding이 유효하지 않습니다.",
        ));
    }
    let manifest_relative = subagent::normalize_relative_path(manifest_path)?;
    let manifest_path = paths::project_root().join(&manifest_relative);
    let body =
        state::read_regular_file_bounded(&manifest_path, MAX_MANIFEST_BYTES, "team manifest")?;
    let manifest = parse_manifest(&body)?;
    if manifest.parent_workflow_id != parent.workflow_id {
        return Err(AppError::blocked(format!(
            "team plan parent binding 불일치\n- active: {}\n- manifest: {}",
            parent.workflow_id, manifest.parent_workflow_id
        )));
    }
    install_manifest(&manifest)?;
    let record = if paths::project_team_file(&manifest.team_id).exists() {
        let existing = load_state(&manifest.team_id)?;
        if existing.manifest_hash != manifest.artifact_hash
            || existing.project_id != identity.project_id
            || existing.session_id != identity.session_id
            || existing.parent_workflow_id != parent.workflow_id
        {
            return Err(AppError::blocked(
                "team plan 재시도 binding이 기존 durable state와 다릅니다.",
            ));
        }
        if existing.stage != TeamStage::Plan {
            return Err(AppError::blocked(format!(
                "team plan은 이미 다음 stage로 전진했습니다.\n- team id: {}\n- current stage: {}",
                existing.team_id,
                existing.stage.as_str()
            )));
        }
        existing
    } else {
        create_state(TeamStateV1::new(&identity, &parent, &manifest)?)?
    };
    append_planned_event_if_missing(&identity, &record)?;
    Ok(format!(
        "team plan\n- status: planned\n- team id: {}\n- parent workflow: {}\n- parent revision: {}\n- stage: {}\n- state revision: {}\n- manifest: {}\n- manifest hash: {}\n- members: {}\n- requested lanes: {}\n- write policy: {}\n- merge policy: {}\n- stop gate: {}\n- boundary: durable plan only; no worker was started and no team stage beyond team-plan was entered.",
        record.team_id,
        record.parent_workflow_id,
        record.parent_revision,
        record.stage.as_str(),
        record.revision,
        manifest_relative,
        record.manifest_hash,
        record.member_count,
        record.requested_lanes,
        manifest.write_policy,
        manifest.merge_policy,
        manifest.stop_gate,
    ))
}

pub fn parse_manifest(body: &str) -> Result<TeamManifestV1, AppError> {
    if body.is_empty() || body.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(AppError::blocked("team manifest 크기 상한 위반"));
    }
    let value = strict_json::parse_value(body, "team manifest")?;
    if strict_json::render_compact(&value) != body {
        return Err(AppError::blocked(
            "team manifest는 canonical compact JSON이어야 합니다.",
        ));
    }
    let strict_json::Value::Object(object) = value else {
        return Err(AppError::blocked("team manifest root는 object여야 합니다."));
    };
    require_keys(&object, MANIFEST_KEYS, "team manifest")?;
    if strict_json::number(&object, "schema_version", "team manifest")? != TEAM_SCHEMA_VERSION {
        return Err(AppError::blocked(
            "지원하지 않는 team manifest schema입니다.",
        ));
    }
    let team_id = strict_json::string(&object, "team_id", "team manifest")?;
    validate_id(&team_id, "team id", MAX_TEAM_ID_BYTES)?;
    let parent_workflow_id = strict_json::string(&object, "parent_workflow_id", "team manifest")?;
    if !parent_workflow_id.starts_with("workflow-") {
        return Err(AppError::blocked(
            "team manifest parent workflow id 형식 오류",
        ));
    }
    let members = parse_members(&object)?;
    let write_policy = strict_json::string(&object, "write_policy", "team manifest")?;
    let merge_policy = strict_json::string(&object, "merge_policy", "team manifest")?;
    let stop_gate = strict_json::string(&object, "stop_gate", "team manifest")?;
    if write_policy != "single_writer"
        || merge_policy != "runtime_owned"
        || stop_gate != "evidence_required"
    {
        return Err(AppError::blocked(
            "team manifest policy는 single_writer/runtime_owned/evidence_required 고정값이어야 합니다.",
        ));
    }
    validate_member_set(&members)?;
    Ok(TeamManifestV1 {
        team_id,
        parent_workflow_id,
        members,
        write_policy,
        merge_policy,
        stop_gate,
        artifact_hash: state::sha256_text(body),
        canonical_body: body.to_string(),
    })
}

pub fn create_state(record: TeamStateV1) -> Result<TeamStateV1, AppError> {
    checkpoint_state(record, 0)
}

pub fn checkpoint_state(
    mut next: TeamStateV1,
    expected_revision: u64,
) -> Result<TeamStateV1, AppError> {
    validate_id(&next.team_id, "team id", MAX_TEAM_ID_BYTES)?;
    let _guard =
        lease::RecoverableLease::acquire(paths::project_team_lock(&next.team_id), "team state")?;
    let current_path = paths::project_team_file(&next.team_id);
    if expected_revision == 0 {
        if current_path.exists() {
            return Err(AppError::blocked("team state create 충돌"));
        }
        if next.revision != 0 || !next.artifact_hash.is_empty() {
            return Err(AppError::blocked(
                "새 team state는 revision 0과 빈 artifact hash에서 시작해야 합니다.",
            ));
        }
        next.revision = 1;
        next.previous_hash = "none".to_string();
    } else {
        let current = load_state_unlocked(&next.team_id)?;
        if current.revision != expected_revision
            || next.revision != current.revision
            || next.artifact_hash != current.artifact_hash
        {
            return Err(AppError::blocked("team state stale revision 차단"));
        }
        if !current.stage.permits(next.stage) {
            return Err(AppError::blocked(format!(
                "team stage 전이 차단\n- current: {}\n- next: {}",
                current.stage.as_str(),
                next.stage.as_str()
            )));
        }
        if immutable_binding_changed(&current, &next) {
            return Err(AppError::blocked("team immutable binding 변경 차단"));
        }
        next.revision = current
            .revision
            .checked_add(1)
            .ok_or_else(|| AppError::blocked("team state revision overflow"))?;
        next.previous_hash = current.artifact_hash;
    }
    if next.revision > MAX_STATE_REVISIONS {
        return Err(AppError::blocked("team state revision 상한 초과"));
    }
    next.artifact_hash = state::sha256_text(&render_payload(&next));
    validate_state(&next, true)?;
    let body = render_state(&next);
    install_snapshot(&next, &body)?;
    state::atomic_replace_bytes(&current_path, body.as_bytes())?;
    let installed = load_state_unlocked(&next.team_id)?;
    if installed != next {
        return Err(AppError::blocked("team canonical state install 검증 실패"));
    }
    Ok(installed)
}

pub fn load_state(team_id: &str) -> Result<TeamStateV1, AppError> {
    validate_id(team_id, "team id", MAX_TEAM_ID_BYTES)?;
    let path = paths::project_team_file(team_id);
    let before = state::read_regular_file_bounded(&path, MAX_STATE_BYTES, "team state")?;
    let record = parse_state(&before)?;
    verify_snapshot_chain(&record, &before)?;
    let after = state::read_regular_file_bounded(&path, MAX_STATE_BYTES, "team state")?;
    if after != before {
        return Err(AppError::blocked(
            "team state가 read 중 변경되어 결과를 폐기합니다.",
        ));
    }
    Ok(record)
}

pub fn latest_for_parent(parent_workflow_id: &str) -> Result<Option<TeamStateV1>, AppError> {
    let identity = ledger::validated_current_identity()?;
    let dir = paths::project_teams_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::blocked(format!(
                "team state directory 읽기 실패: {error}"
            )))
        }
    };
    let mut team_ids = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            AppError::blocked(format!("team state directory entry 읽기 실패: {error}"))
        })?;
        let file_type = entry
            .file_type()
            .map_err(|error| AppError::blocked(format!("team state type 확인 실패: {error}")))?;
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !name.ends_with(".json") || name.ends_with(".manifest.json") {
            continue;
        }
        team_ids.push(name.trim_end_matches(".json").to_string());
        if team_ids.len() > MAX_TEAM_RECORDS {
            return Err(AppError::blocked("team state record 수 상한 초과"));
        }
    }
    let mut records = Vec::new();
    for team_id in team_ids {
        let record = load_state(&team_id)?;
        if record.project_id == identity.project_id
            && record.session_id == identity.session_id
            && record.parent_workflow_id == parent_workflow_id
        {
            records.push(record);
        }
    }
    records.sort_by(|left, right| {
        (left.updated_at_ms, left.revision, left.team_id.as_str()).cmp(&(
            right.updated_at_ms,
            right.revision,
            right.team_id.as_str(),
        ))
    });
    Ok(records.pop())
}

fn parse_members(object: &strict_json::Object) -> Result<Vec<TeamMemberV1>, AppError> {
    let Some(strict_json::Value::Array(values)) = object.get("members") else {
        return Err(AppError::blocked(
            "team manifest members는 array여야 합니다.",
        ));
    };
    if values.is_empty() || values.len() > MAX_TEAM_LANES {
        return Err(AppError::blocked(format!(
            "team manifest member 수는 1..={MAX_TEAM_LANES} 범위여야 합니다."
        )));
    }
    values
        .iter()
        .map(|value| {
            let strict_json::Value::Object(member) = value else {
                return Err(AppError::blocked("team member는 object여야 합니다."));
            };
            require_keys(member, MEMBER_KEYS, "team member")?;
            let lane = u32::try_from(strict_json::number(member, "lane", "team member")?)
                .map_err(|_| AppError::blocked("team member lane 범위 오류"))?;
            let member_id = strict_json::string(member, "id", "team member")?;
            validate_id(&member_id, "team member id", MAX_MEMBER_ID_BYTES)?;
            let role = strict_json::string(member, "role", "team member")?;
            let task = strict_json::string(member, "task", "team member")?;
            let tools = string_array(member, "tools", "team member")?;
            let read_paths = string_array(member, "read_paths", "team member")?;
            let write_paths = string_array(member, "write_paths", "team member")?;
            let timeout_ms =
                u32::try_from(strict_json::number(member, "timeout_ms", "team member")?)
                    .map_err(|_| AppError::blocked("team member timeout 범위 오류"))?;
            let max_tokens =
                u32::try_from(strict_json::number(member, "max_tokens", "team member")?)
                    .map_err(|_| AppError::blocked("team member max token 범위 오류"))?;
            let launch = subagent::validate_launch(
                &role,
                &task,
                &tools,
                &read_paths,
                &write_paths,
                Some(timeout_ms),
                Some(max_tokens),
            )?;
            Ok(TeamMemberV1 {
                lane,
                member_id,
                role: launch.role.as_str().to_string(),
                task,
                task_hash: launch.task_hash,
                tools: launch.declared_tools,
                read_paths: launch.read_paths,
                write_paths: launch.write_paths,
                timeout_ms: launch.timeout_ms,
                max_tokens: launch.requested_max_tokens,
            })
        })
        .collect()
}

fn validate_member_set(members: &[TeamMemberV1]) -> Result<(), AppError> {
    let mut ids = BTreeSet::new();
    let mut ownership = BTreeMap::<String, u32>::new();
    for (index, member) in members.iter().enumerate() {
        let expected_lane = (index + 1) as u32;
        if member.lane != expected_lane {
            return Err(AppError::blocked(format!(
                "team member lane은 1부터 순서대로 선언해야 합니다: expected={expected_lane} actual={}",
                member.lane
            )));
        }
        if !ids.insert(member.member_id.as_str()) {
            return Err(AppError::blocked("team member id 중복 차단"));
        }
        for path in &member.write_paths {
            if let Some(owner) = ownership.insert(path.clone(), member.lane) {
                return Err(AppError::blocked(format!(
                    "team manifest cross-lane ownership 충돌\n- path: {path}\n- lanes: {owner}, {}",
                    member.lane
                )));
            }
        }
    }
    Ok(())
}

fn install_manifest(manifest: &TeamManifestV1) -> Result<(), AppError> {
    fs::create_dir_all(paths::project_teams_dir())
        .map_err(|error| AppError::runtime(format!("team directory 생성 실패: {error}")))?;
    let path = paths::project_team_manifest_file(&manifest.team_id);
    if path.exists() {
        let installed =
            state::read_regular_file_bounded(&path, MAX_MANIFEST_BYTES, "team manifest")?;
        if installed != manifest.canonical_body {
            return Err(AppError::blocked(
                "기존 team manifest artifact binding 불일치",
            ));
        }
        return Ok(());
    }
    state::atomic_replace_bytes(&path, manifest.canonical_body.as_bytes())?;
    let installed = state::read_regular_file_bounded(&path, MAX_MANIFEST_BYTES, "team manifest")?;
    if installed != manifest.canonical_body
        || state::sha256_text(&installed) != manifest.artifact_hash
    {
        return Err(AppError::blocked("team manifest install 검증 실패"));
    }
    Ok(())
}

fn load_state_unlocked(team_id: &str) -> Result<TeamStateV1, AppError> {
    let body = state::read_regular_file_bounded(
        &paths::project_team_file(team_id),
        MAX_STATE_BYTES,
        "team state",
    )?;
    parse_state(&body)
}

fn install_snapshot(record: &TeamStateV1, body: &str) -> Result<(), AppError> {
    fs::create_dir_all(paths::project_team_snapshots_dir(&record.team_id)).map_err(|error| {
        AppError::runtime(format!("team snapshot directory 생성 실패: {error}"))
    })?;
    let path = paths::project_team_snapshot_file(&record.team_id, record.revision);
    if path.exists() {
        let existing = state::read_regular_file_bounded(&path, MAX_STATE_BYTES, "team snapshot")?;
        if existing != body {
            return Err(AppError::blocked("team snapshot revision 충돌"));
        }
        return Ok(());
    }
    state::atomic_replace_bytes(&path, body.as_bytes())
}

fn verify_snapshot_chain(record: &TeamStateV1, current_body: &str) -> Result<(), AppError> {
    if record.revision == 0 || record.revision > MAX_STATE_REVISIONS {
        return Err(AppError::blocked("team snapshot revision 범위 오류"));
    }
    let mut previous = "none".to_string();
    for revision in 1..=record.revision {
        let body = state::read_regular_file_bounded(
            &paths::project_team_snapshot_file(&record.team_id, revision),
            MAX_STATE_BYTES,
            "team snapshot",
        )?;
        let snapshot = parse_state(&body)?;
        if snapshot.revision != revision
            || snapshot.previous_hash != previous
            || immutable_binding_changed(record, &snapshot)
        {
            return Err(AppError::blocked("team snapshot hash chain 불일치"));
        }
        previous = snapshot.artifact_hash;
        if revision == record.revision && body != current_body {
            return Err(AppError::blocked(
                "team current state/snapshot binding 불일치",
            ));
        }
    }
    Ok(())
}

fn render_payload(record: &TeamStateV1) -> String {
    format!(
        "{{\"schema_version\":{TEAM_SCHEMA_VERSION},\"team_id\":\"{}\",\"revision\":{},\"previous_hash\":\"{}\",\"manifest_hash\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"parent_revision\":{},\"parent_artifact_hash\":\"{}\",\"stage\":\"{}\",\"status\":\"{}\",\"requested_lanes\":{},\"admitted_lanes\":{},\"execution_mode\":\"{}\",\"member_count\":{},\"created_at_ms\":{},\"updated_at_ms\":{}}}",
        ledger::json_string(&record.team_id),
        record.revision,
        ledger::json_string(&record.previous_hash),
        ledger::json_string(&record.manifest_hash),
        ledger::json_string(&record.project_id),
        ledger::json_string(&record.session_id),
        ledger::json_string(&record.parent_workflow_id),
        record.parent_revision,
        ledger::json_string(&record.parent_artifact_hash),
        record.stage.as_str(),
        record.status,
        record.requested_lanes,
        record.admitted_lanes,
        record.execution_mode,
        record.member_count,
        record.created_at_ms,
        record.updated_at_ms,
    )
}

fn render_state(record: &TeamStateV1) -> String {
    format!(
        "{{\"schema_version\":{TEAM_SCHEMA_VERSION},\"team_id\":\"{}\",\"revision\":{},\"previous_hash\":\"{}\",\"artifact_hash\":\"{}\",\"manifest_hash\":\"{}\",\"project_id\":\"{}\",\"session_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"parent_revision\":{},\"parent_artifact_hash\":\"{}\",\"stage\":\"{}\",\"status\":\"{}\",\"requested_lanes\":{},\"admitted_lanes\":{},\"execution_mode\":\"{}\",\"member_count\":{},\"created_at_ms\":{},\"updated_at_ms\":{}}}",
        ledger::json_string(&record.team_id),
        record.revision,
        ledger::json_string(&record.previous_hash),
        ledger::json_string(&record.artifact_hash),
        ledger::json_string(&record.manifest_hash),
        ledger::json_string(&record.project_id),
        ledger::json_string(&record.session_id),
        ledger::json_string(&record.parent_workflow_id),
        record.parent_revision,
        ledger::json_string(&record.parent_artifact_hash),
        record.stage.as_str(),
        record.status,
        record.requested_lanes,
        record.admitted_lanes,
        record.execution_mode,
        record.member_count,
        record.created_at_ms,
        record.updated_at_ms,
    )
}

fn parse_state(body: &str) -> Result<TeamStateV1, AppError> {
    let object = strict_json::parse_object_exact_order(body, STATE_KEYS, "team state")?;
    if strict_json::number(&object, "schema_version", "team state")? != TEAM_SCHEMA_VERSION {
        return Err(AppError::blocked("지원하지 않는 team state schema"));
    }
    let stage_value = strict_json::string(&object, "stage", "team state")?;
    let stage = TeamStage::parse(&stage_value)
        .ok_or_else(|| AppError::blocked("team state stage 형식 오류"))?;
    let record = TeamStateV1 {
        team_id: strict_json::string(&object, "team_id", "team state")?,
        revision: strict_json::number(&object, "revision", "team state")?,
        previous_hash: strict_json::string(&object, "previous_hash", "team state")?,
        artifact_hash: strict_json::string(&object, "artifact_hash", "team state")?,
        manifest_hash: strict_json::string(&object, "manifest_hash", "team state")?,
        project_id: strict_json::string(&object, "project_id", "team state")?,
        session_id: strict_json::string(&object, "session_id", "team state")?,
        parent_workflow_id: strict_json::string(&object, "parent_workflow_id", "team state")?,
        parent_revision: strict_json::number(&object, "parent_revision", "team state")?,
        parent_artifact_hash: strict_json::string(&object, "parent_artifact_hash", "team state")?,
        stage,
        status: strict_json::string(&object, "status", "team state")?,
        requested_lanes: u32::try_from(strict_json::number(
            &object,
            "requested_lanes",
            "team state",
        )?)
        .map_err(|_| AppError::blocked("team state requested lane 범위 오류"))?,
        admitted_lanes: u32::try_from(strict_json::number(
            &object,
            "admitted_lanes",
            "team state",
        )?)
        .map_err(|_| AppError::blocked("team state admitted lane 범위 오류"))?,
        execution_mode: strict_json::string(&object, "execution_mode", "team state")?,
        member_count: u32::try_from(strict_json::number(&object, "member_count", "team state")?)
            .map_err(|_| AppError::blocked("team state member count 범위 오류"))?,
        created_at_ms: strict_json::number_u128(&object, "created_at_ms", "team state")?,
        updated_at_ms: strict_json::number_u128(&object, "updated_at_ms", "team state")?,
    };
    validate_state(&record, true)?;
    Ok(record)
}

fn validate_state(record: &TeamStateV1, installed: bool) -> Result<(), AppError> {
    validate_id(&record.team_id, "team id", MAX_TEAM_ID_BYTES)?;
    if record.revision == 0
        || record.revision > MAX_STATE_REVISIONS
        || !is_sha256(&record.manifest_hash)
        || record.project_id.is_empty()
        || record.session_id.is_empty()
        || !record.parent_workflow_id.starts_with("workflow-")
        || record.parent_revision == 0
        || !is_sha256(&record.parent_artifact_hash)
        || record.requested_lanes == 0
        || record.requested_lanes as usize > MAX_TEAM_LANES
        || record.member_count != record.requested_lanes
        || record.admitted_lanes > record.requested_lanes
        || record.created_at_ms == 0
        || record.updated_at_ms < record.created_at_ms
    {
        return Err(AppError::blocked("team state invariant 위반"));
    }
    let expected_status = match record.stage {
        TeamStage::Complete => "completed",
        TeamStage::Failed => "failed",
        TeamStage::Cancelled => "cancelled",
        _ => "active",
    };
    if record.status != expected_status {
        return Err(AppError::blocked("team state status/stage binding 불일치"));
    }
    if record.stage == TeamStage::Plan {
        if record.admitted_lanes != 0 || record.execution_mode != "pending" {
            return Err(AppError::blocked("team plan admission binding 불일치"));
        }
    } else if !record.stage.is_terminal()
        && (record.admitted_lanes == 0
            || !matches!(record.execution_mode.as_str(), "parallel" | "sequential"))
    {
        return Err(AppError::blocked(
            "team active stage admission binding 불일치",
        ));
    }
    if installed
        && (!is_sha256(&record.artifact_hash)
            || state::sha256_text(&render_payload(record)) != record.artifact_hash)
    {
        return Err(AppError::blocked("team state artifact hash 불일치"));
    }
    Ok(())
}

fn immutable_binding_changed(left: &TeamStateV1, right: &TeamStateV1) -> bool {
    left.team_id != right.team_id
        || left.manifest_hash != right.manifest_hash
        || left.project_id != right.project_id
        || left.session_id != right.session_id
        || left.parent_workflow_id != right.parent_workflow_id
        || left.parent_revision != right.parent_revision
        || left.parent_artifact_hash != right.parent_artifact_hash
        || left.requested_lanes != right.requested_lanes
        || left.member_count != right.member_count
        || left.created_at_ms != right.created_at_ms
}

fn append_planned_event_if_missing(
    identity: &ledger::RuntimeIdentity,
    record: &TeamStateV1,
) -> Result<(), AppError> {
    if ledger::event_details_match(
        "team.stage.planned",
        &[("team_id", record.team_id.as_str()), ("revision", "1")],
    )? {
        return Ok(());
    }
    let event = ledger::new_event_for(
        identity,
        "team.stage.planned",
        "team plan recorded",
        &format!(
            "team_id={} revision={} stage={} parent_workflow_id={} member_count={} manifest_hash={}",
            record.team_id,
            record.revision,
            record.stage.as_str(),
            record.parent_workflow_id,
            record.member_count,
            record.manifest_hash,
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)
}

fn require_keys(
    object: &strict_json::Object,
    expected: &[&str],
    context: &str,
) -> Result<(), AppError> {
    let actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    if actual != expected {
        return Err(AppError::blocked(format!(
            "{context} key order/schema 불일치"
        )));
    }
    Ok(())
}

fn string_array(
    object: &strict_json::Object,
    key: &str,
    context: &str,
) -> Result<Vec<String>, AppError> {
    let Some(strict_json::Value::Array(values)) = object.get(key) else {
        return Err(AppError::blocked(format!("{context}: {key} array 필요")));
    };
    values
        .iter()
        .map(|value| match value {
            strict_json::Value::String(value) => Ok(value.clone()),
            _ => Err(AppError::blocked(format!(
                "{context}: {key} item type 오류"
            ))),
        })
        .collect()
}

fn validate_id(value: &str, label: &str, max_bytes: usize) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err(AppError::blocked(format!("{label} 형식 오류: {value}")));
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
        .map_err(|_| AppError::runtime("team system clock 오류"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialize_parent() -> state::WorkflowRecord {
        fs::create_dir_all(paths::project_root().join("src")).unwrap();
        fs::write(paths::project_root().join("src/main.rs"), "fn main() {}\n").unwrap();
        state::initialize().unwrap();
        state::create_workflow("team parent fixture").unwrap()
    }

    fn manifest(parent: &state::WorkflowRecord, duplicate_write: bool) -> String {
        let (second_role, second_tools, second_write) = if duplicate_write {
            (
                "executor",
                "[\"read_file\",\"render_diff\"]",
                "[\"src/main.rs\"]",
            )
        } else {
            ("verifier", "[\"read_file\"]", "[]")
        };
        format!(
            "{{\"schema_version\":1,\"team_id\":\"team-fixture\",\"parent_workflow_id\":\"{}\",\"members\":[{{\"lane\":1,\"id\":\"executor-1\",\"role\":\"executor\",\"task\":\"prepare a bounded diff\",\"tools\":[\"read_file\",\"render_diff\"],\"read_paths\":[\"src/main.rs\"],\"write_paths\":[\"src/main.rs\"],\"timeout_ms\":30000,\"max_tokens\":256}},{{\"lane\":2,\"id\":\"verifier-1\",\"role\":\"{}\",\"task\":\"verify the bounded result\",\"tools\":{},\"read_paths\":[\"src/main.rs\"],\"write_paths\":{},\"timeout_ms\":30000,\"max_tokens\":256}}],\"write_policy\":\"single_writer\",\"merge_policy\":\"runtime_owned\",\"stop_gate\":\"evidence_required\"}}",
            parent.workflow_id, second_role, second_tools, second_write,
        )
    }

    #[test]
    fn plan_persists_canonical_manifest_and_hash_chained_state() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let body = manifest(&parent, false);
        fs::write(paths::project_root().join("team.json"), &body).unwrap();

        let report = plan_report("team.json").unwrap();
        let record = load_state("team-fixture").unwrap();
        let latest = latest_for_parent(&parent.workflow_id).unwrap().unwrap();
        let retry = plan_report("team.json").unwrap();
        let status = crate::team::status_report().unwrap();
        let planned_events = ledger::read_runtime_events()
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "team.stage.planned")
            .count();

        assert!(report.contains("status: planned"));
        assert!(retry.contains("status: planned"));
        assert_eq!(planned_events, 1);
        assert!(status.contains("current team id: team-fixture"));
        assert!(status.contains("current team stage: team-plan"));
        assert!(report.contains("stage: team-plan"));
        assert_eq!(record.revision, 1);
        assert_eq!(record.previous_hash, "none");
        assert_eq!(record.manifest_hash, state::sha256_text(&body));
        assert_eq!(record, latest);
        assert_eq!(
            state::read_regular_file_bounded(
                &paths::project_team_manifest_file("team-fixture"),
                MAX_MANIFEST_BYTES,
                "test manifest",
            )
            .unwrap(),
            body
        );
    }

    #[test]
    fn manifest_rejects_parent_mismatch_and_cross_lane_ownership() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let mismatch = manifest(&parent, false).replace(&parent.workflow_id, "workflow-other");
        fs::write(paths::project_root().join("mismatch.json"), mismatch).unwrap();
        assert!(plan_report("mismatch.json")
            .unwrap_err()
            .message
            .contains("parent binding"));

        let conflict = manifest(&parent, true);
        assert!(parse_manifest(&conflict)
            .unwrap_err()
            .message
            .contains("ownership 충돌"));
    }

    #[test]
    fn stage_machine_allows_only_ordered_runtime_transitions() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let body = manifest(&parent, false);
        fs::write(paths::project_root().join("team.json"), body).unwrap();
        plan_report("team.json").unwrap();

        let planned = load_state("team-fixture").unwrap();
        let mut invalid = planned.clone();
        assert!(invalid
            .transition_to(TeamStage::Execute, None, None)
            .unwrap_err()
            .message
            .contains("stage 전이 차단"));

        let mut dispatched = planned.clone();
        dispatched
            .transition_to(TeamStage::Dispatch, Some(2), Some("parallel"))
            .unwrap();
        let dispatched = checkpoint_state(dispatched, planned.revision).unwrap();
        let mut executing = dispatched.clone();
        executing
            .transition_to(TeamStage::Execute, None, None)
            .unwrap();
        let executing = checkpoint_state(executing, dispatched.revision).unwrap();

        assert_eq!(executing.stage, TeamStage::Execute);
        assert_eq!(executing.revision, 3);
        assert_eq!(executing.admitted_lanes, 2);
        assert_eq!(executing.execution_mode, "parallel");
        assert_eq!(load_state("team-fixture").unwrap(), executing);
    }

    #[test]
    fn tampered_current_state_is_rejected_against_artifact_hash() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        fs::write(
            paths::project_root().join("team.json"),
            manifest(&parent, false),
        )
        .unwrap();
        plan_report("team.json").unwrap();

        let path = paths::project_team_file("team-fixture");
        let tampered = fs::read_to_string(&path)
            .unwrap()
            .replace("\"status\":\"active\"", "\"status\":\"failed\"");
        fs::write(path, tampered).unwrap();

        assert!(load_state("team-fixture")
            .unwrap_err()
            .message
            .contains("status/stage binding"));
    }
}
