use crate::app::observability_adapter as observability;
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::collaboration::team_state::{
    immutable_binding_changed, is_sha256, parse_state, render_payload, render_state, validate_id,
    validate_state, MAX_MANIFEST_BYTES, MAX_STATE_BYTES, MAX_STATE_REVISIONS, MAX_TEAM_ID_BYTES,
    TEAM_SCHEMA_VERSION,
};
pub(crate) use crate::runtime_core::collaboration::team_state::{
    parse_manifest, TeamManifestV1, TeamStage, TeamStateV1,
};
use crate::{
    adapters::filesystem::layout as paths, adapters::filesystem::lease, ledger, state, subagent,
};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_CANCEL_MARKER_BYTES: u64 = 4_096;
const MAX_TEAM_RECORDS: usize = 256;

const CANCEL_MARKER_KEYS: &[&str] = &[
    "schema_version",
    "team_id",
    "manifest_hash",
    "parent_workflow_id",
    "requested_at_ms",
];

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

fn install_cancel_marker(record: &TeamStateV1) -> Result<(), AppError> {
    let path = paths::project_team_cancel_file(&record.team_id);
    if path.exists() {
        let body = state::read_regular_file_bounded(
            &path,
            MAX_CANCEL_MARKER_BYTES,
            "team cancellation marker",
        )?;
        let (team_id, manifest_hash, parent_workflow_id) = parse_cancel_marker(&body)?;
        if team_id == record.team_id
            && manifest_hash == record.manifest_hash
            && parent_workflow_id == record.parent_workflow_id
        {
            return Ok(());
        }
        return Err(AppError::blocked(
            "기존 team cancellation marker binding 불일치",
        ));
    }
    let body = format!(
        "{{\"schema_version\":1,\"team_id\":\"{}\",\"manifest_hash\":\"{}\",\"parent_workflow_id\":\"{}\",\"requested_at_ms\":{}}}",
        ledger::json_string(&record.team_id),
        ledger::json_string(&record.manifest_hash),
        ledger::json_string(&record.parent_workflow_id),
        now_ms()?,
    );
    state::atomic_replace_bytes(&path, body.as_bytes())?;
    let installed = state::read_regular_file_bounded(
        &path,
        MAX_CANCEL_MARKER_BYTES,
        "team cancellation marker",
    )?;
    if installed != body {
        return Err(AppError::blocked(
            "team cancellation marker install 검증 실패",
        ));
    }
    Ok(())
}

fn parse_cancel_marker(body: &str) -> Result<(String, String, String), AppError> {
    let value = strict_json::parse_value(body, "team cancellation marker")?;
    if strict_json::render_compact(&value) != body {
        return Err(AppError::blocked(
            "team cancellation marker canonical JSON 불일치",
        ));
    }
    let strict_json::Value::Object(object) = value else {
        return Err(AppError::blocked("team cancellation marker root type 오류"));
    };
    require_keys(&object, CANCEL_MARKER_KEYS, "team cancellation marker")?;
    if strict_json::number(&object, "schema_version", "team cancellation marker")?
        != TEAM_SCHEMA_VERSION
    {
        return Err(AppError::blocked(
            "team cancellation marker schema version 불일치",
        ));
    }
    let team_id = strict_json::string(&object, "team_id", "team cancellation marker")?;
    validate_id(&team_id, "team id", MAX_TEAM_ID_BYTES)?;
    let manifest_hash = strict_json::string(&object, "manifest_hash", "team cancellation marker")?;
    if manifest_hash.len() != 64 || !manifest_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::blocked(
            "team cancellation marker manifest hash 형식 오류",
        ));
    }
    let parent_workflow_id =
        strict_json::string(&object, "parent_workflow_id", "team cancellation marker")?;
    if !parent_workflow_id.starts_with("workflow-") {
        return Err(AppError::blocked(
            "team cancellation marker parent workflow 형식 오류",
        ));
    }
    strict_json::number(&object, "requested_at_ms", "team cancellation marker")?;
    Ok((team_id, manifest_hash, parent_workflow_id))
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

fn append_stage_event_if_missing(
    identity: &ledger::RuntimeIdentity,
    record: &TeamStateV1,
) -> Result<(), AppError> {
    let event_type = match record.stage {
        TeamStage::Plan => "team.stage.planned",
        TeamStage::Dispatch => "team.stage.dispatched",
        TeamStage::Execute => "team.stage.executing",
        TeamStage::Review => "team.stage.reviewing",
        TeamStage::Verify => "team.stage.verifying",
        TeamStage::Merge => "team.stage.merging",
        TeamStage::Report => "team.stage.reporting",
        TeamStage::Complete => "team.stage.completed",
        TeamStage::Failed => "team.stage.failed",
        TeamStage::Cancelled => "team.stage.cancelled",
    };
    if ledger::event_details_match(
        event_type,
        &[
            ("team_id", record.team_id.as_str()),
            ("stage", record.stage.as_str()),
        ],
    )? {
        return Ok(());
    }
    let event = ledger::new_event_for(
        identity,
        event_type,
        "team stage advanced",
        &format!(
            "team_id={} revision={} stage={} status={} requested_lanes={} admitted_lanes={} execution_mode={}",
            record.team_id,
            record.revision,
            record.stage.as_str(),
            record.status,
            record.requested_lanes,
            record.admitted_lanes,
            record.execution_mode,
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

        let ancestor_conflict = conflict.replacen(
            "\"write_paths\":[\"src/main.rs\"]",
            "\"write_paths\":[\"src\"]",
            1,
        );
        assert!(parse_manifest(&ancestor_conflict)
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
    fn cancellation_marker_is_durable_idempotent_and_hash_bound() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let parent = initialize_parent();
        let body = manifest(&parent, false);
        fs::write(paths::project_root().join("team.json"), body).unwrap();
        plan_report("team.json").unwrap();

        let report = cancel_report("team-fixture").unwrap();
        let retry = cancel_report("team-fixture").unwrap();
        let cancelled = load_state("team-fixture").unwrap();

        assert!(report.contains("status: cancellation-requested"));
        assert!(retry.contains("status: already-cancelled"));
        assert_eq!(cancelled.stage, TeamStage::Cancelled);
        assert!(cancellation_requested("team-fixture").unwrap());

        let marker_path = paths::project_team_cancel_file("team-fixture");
        let marker = fs::read_to_string(&marker_path).unwrap();
        fs::write(
            &marker_path,
            marker.replace(&cancelled.manifest_hash, &"0".repeat(64)),
        )
        .unwrap();
        assert!(cancellation_requested("team-fixture")
            .unwrap_err()
            .message
            .contains("immutable binding"));
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
