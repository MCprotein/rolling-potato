use super::{now_ms, TeamManifestV1, TeamStateV1};
use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::collaboration::team_state::{
    immutable_binding_changed, parse_state, validate_id, MAX_MANIFEST_BYTES, MAX_STATE_BYTES,
    MAX_STATE_REVISIONS, MAX_TEAM_ID_BYTES, TEAM_SCHEMA_VERSION,
};
use std::fs;

pub(super) const MAX_CANCEL_MARKER_BYTES: u64 = 4_096;

const CANCEL_MARKER_KEYS: &[&str] = &[
    "schema_version",
    "team_id",
    "manifest_hash",
    "parent_workflow_id",
    "requested_at_ms",
];

pub(super) fn install_cancel_marker(record: &TeamStateV1) -> Result<(), AppError> {
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
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, body.as_bytes())?;
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

pub(super) fn parse_cancel_marker(body: &str) -> Result<(String, String, String), AppError> {
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

pub(super) fn install_manifest(manifest: &TeamManifestV1) -> Result<(), AppError> {
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
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &path,
        manifest.canonical_body.as_bytes(),
    )?;
    let installed = state::read_regular_file_bounded(&path, MAX_MANIFEST_BYTES, "team manifest")?;
    if installed != manifest.canonical_body
        || state::sha256_text(&installed) != manifest.artifact_hash
    {
        return Err(AppError::blocked("team manifest install 검증 실패"));
    }
    Ok(())
}

pub(super) fn load_state_unlocked(team_id: &str) -> Result<TeamStateV1, AppError> {
    let body = state::read_regular_file_bounded(
        &paths::project_team_file(team_id),
        MAX_STATE_BYTES,
        "team state",
    )?;
    parse_state(&body)
}

pub(super) fn install_snapshot(record: &TeamStateV1, body: &str) -> Result<(), AppError> {
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
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, body.as_bytes())
}

pub(super) fn verify_snapshot_chain(
    record: &TeamStateV1,
    current_body: &str,
) -> Result<(), AppError> {
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
