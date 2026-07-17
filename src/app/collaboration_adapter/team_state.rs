use crate::app::collaboration_adapter::subagent;
use crate::app::observability_adapter as observability;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team_state::{
    immutable_binding_changed, is_sha256, parse_state, render_payload, render_state, validate_id,
    validate_state, MAX_MANIFEST_BYTES, MAX_STATE_BYTES, MAX_STATE_REVISIONS, MAX_TEAM_ID_BYTES,
};
pub(crate) use crate::runtime_core::collaboration::team_state::{
    parse_manifest, TeamManifestV1, TeamStage, TeamStateV1,
};
use crate::adapters::filesystem::layout as paths;
use crate::adapters::filesystem::lease;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_TEAM_RECORDS: usize = 256;

mod events;
mod persistence;

use events::{append_planned_event_if_missing, append_stage_event_if_missing};
use persistence::{
    install_cancel_marker, install_manifest, install_snapshot, load_state_unlocked,
    parse_cancel_marker, verify_snapshot_chain, MAX_CANCEL_MARKER_BYTES,
};

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
        self.transition_to_at(next, admitted_lanes, execution_mode, now_ms()?)
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

pub fn load_manifest(team_id: &str) -> Result<TeamManifestV1, AppError> {
    validate_id(team_id, "team id", MAX_TEAM_ID_BYTES)?;
    let body = state::read_regular_file_bounded(
        &paths::project_team_manifest_file(team_id),
        MAX_MANIFEST_BYTES,
        "team manifest",
    )?;
    let manifest = parse_manifest(&body)?;
    if manifest.team_id != team_id {
        return Err(AppError::blocked("team manifest identity binding 불일치"));
    }
    Ok(manifest)
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
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &current_path,
        body.as_bytes(),
    )?;
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

pub fn advance_state(
    team_id: &str,
    next: TeamStage,
    admitted_lanes: Option<u32>,
    execution_mode: Option<&str>,
) -> Result<TeamStateV1, AppError> {
    let identity = ledger::validated_current_identity()?;
    let current = load_state(team_id)?;
    if current.project_id != identity.project_id || current.session_id != identity.session_id {
        return Err(AppError::blocked("team stage owner binding 불일치"));
    }
    if current.stage == next {
        if next == TeamStage::Dispatch
            && (current.admitted_lanes != admitted_lanes.unwrap_or(0)
                || current.execution_mode != execution_mode.unwrap_or(""))
        {
            return Err(AppError::blocked(
                "team dispatch stage 재시도 admission binding 불일치",
            ));
        }
        append_stage_event_if_missing(&identity, &current)?;
        return Ok(current);
    }
    let mut next_record = current.clone();
    next_record.transition_to(next, admitted_lanes, execution_mode)?;
    let installed = checkpoint_state(next_record, current.revision)?;
    append_stage_event_if_missing(&identity, &installed)?;
    Ok(installed)
}

pub fn cancel_report(team_id: &str) -> Result<String, AppError> {
    let _operation = lease::RecoverableLease::acquire(
        paths::project_team_operation_lock(team_id),
        "team operation",
    )?;
    let identity = ledger::validated_current_identity()?;
    let current = load_state(team_id)?;
    if current.project_id != identity.project_id || current.session_id != identity.session_id {
        return Err(AppError::blocked("team cancel owner binding 불일치"));
    }
    if current.stage == TeamStage::Cancelled {
        return Ok(format!(
            "team cancel\n- status: already-cancelled\n- team id: {}\n- stage: {}",
            current.team_id,
            current.stage.as_str()
        ));
    }
    if current.stage.is_terminal() {
        return Err(AppError::blocked(format!(
            "team cancel terminal state 차단\n- team id: {}\n- stage: {}",
            current.team_id,
            current.stage.as_str()
        )));
    }
    install_cancel_marker(&current)?;
    let cancelled = advance_state(team_id, TeamStage::Cancelled, None, None)?;
    Ok(format!(
        "team cancel\n- status: cancellation-requested\n- team id: {}\n- stage: {}\n- marker: {}\n- boundary: every active or subsequently admitted team worker observes the same durable marker; no worker result is merged.",
        cancelled.team_id,
        cancelled.stage.as_str(),
        paths::project_team_cancel_file(team_id).display(),
    ))
}

pub fn cancellation_requested(team_id: &str) -> Result<bool, AppError> {
    validate_id(team_id, "team id", MAX_TEAM_ID_BYTES)?;
    let path = paths::project_team_cancel_file(team_id);
    if !path.exists() {
        return Ok(false);
    }
    let body = state::read_regular_file_bounded(
        &path,
        MAX_CANCEL_MARKER_BYTES,
        "team cancellation marker",
    )?;
    let (marker_team_id, manifest_hash, parent_workflow_id) = parse_cancel_marker(&body)?;
    let current = load_state(team_id)?;
    if marker_team_id != current.team_id
        || manifest_hash != current.manifest_hash
        || parent_workflow_id != current.parent_workflow_id
    {
        return Err(AppError::blocked(
            "team cancellation marker immutable binding 불일치",
        ));
    }
    Ok(true)
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
        if !name.ends_with(".json") {
            continue;
        }
        let team_id = name.trim_end_matches(".json");
        if validate_id(team_id, "team id", MAX_TEAM_ID_BYTES).is_err() {
            continue;
        }
        team_ids.push(team_id.to_string());
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

fn now_ms() -> Result<u128, AppError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|_| AppError::runtime("team system clock 오류"))
}

#[cfg(test)]
#[path = "team_state/tests.rs"]
mod tests;
