use super::*;
use std::fs::{File, OpenOptions};
use std::io::Read;

use crate::adapters::filesystem::atomic_write::{replace_file, sync_parent};
#[cfg(windows)]
use crate::adapters::filesystem::windows_replace;

pub(super) fn parse_current_state(
    body: &str,
    context: &str,
) -> Result<CurrentStateSnapshot, AppError> {
    let value = strict_json::parse_value(body, context)?;
    let strict_json::Value::Object(root) = &value else {
        return Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: root must be object"
        )));
    };
    let schema = strict_json::number(root, "schema_version", context)?;
    match schema {
        1 => parse_current_state_v1(body, value, context),
        2 => parse_current_state_v2(body, context),
        _ => Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: unsupported schema version"
        ))),
    }
}

pub(crate) fn validated_identity_from_current_state(
    body: &str,
    fresh: &RuntimeIdentity,
) -> Result<RuntimeIdentity, AppError> {
    let snapshot = parse_current_state(body, "current-state identity")?;
    if snapshot.project_id != fresh.project_id || snapshot.project_root != fresh.project_root {
        return Err(AppError::blocked(
            "current-state identity project binding 불일치",
        ));
    }
    Ok(RuntimeIdentity {
        project_id: snapshot.project_id,
        session_id: snapshot.session_id,
        project_root: snapshot.project_root,
    })
}

pub(crate) fn current_state_lease_view() -> Result<CurrentStateLeaseView, AppError> {
    let identity = ledger::validated_current_identity()?;
    let _transition_guard = transition::TransitionGuard::acquire_for(
        &identity.project_id,
        transition::CurrentStateIntent::RecoverWorkflow,
    )?;
    current_state_lease_view_under_transition()
}

pub(crate) fn tui_state_snapshot_read_only(
    max_ledger_events: usize,
) -> Result<TuiStateSnapshot, AppError> {
    with_validation_gap_writes_suppressed(|| {
        let path = paths::current_state_file();
        let body = read_regular_file_bounded(&path, 128 * 1024, "TUI current-state")?;
        let snapshot = parse_current_state(&body, "TUI current-state read-only")?;
        if snapshot.schema_version != 2 {
            return Err(AppError::blocked(
                "TUI read-only current-state는 schema v2 canonical image가 필요합니다.",
            ));
        }
        let fresh = ledger::fresh_identity();
        let identity = snapshot_domain::validated_tui_identity(&snapshot, &fresh)?;
        let ledger_tail =
            ledger::read_runtime_tail_read_only(max_ledger_events.max(1), 2 * 1024 * 1024)?;
        let current_ledger_binding_stale = snapshot.ledger_binding != ledger_tail.binding;
        snapshot_domain::validate_ledger_ancestor(
            &snapshot.ledger_binding,
            &ledger_tail.binding,
            &ledger_tail.events,
        )?;
        let active_workflow = snapshot
            .active_workflow
            .as_ref()
            .map(|binding| load_workflow_read_only(binding, &identity, &ledger_tail.events))
            .transpose()?;
        Ok(TuiStateSnapshot {
            identity,
            current_revision: snapshot.revision,
            current_hash: snapshot.artifact_hash,
            ledger_binding: ledger_tail.binding,
            ledger_events: ledger_tail.events,
            active_workflow,
            ledger_tail_truncated: ledger_tail.truncated,
            current_ledger_binding_stale,
        })
    })
}

fn load_workflow_read_only(
    binding: &CurrentWorkflowBinding,
    identity: &RuntimeIdentity,
    ledger_events: &[ledger::ParsedLedgerEvent],
) -> Result<WorkflowRecord, AppError> {
    validate_workflow_id(&binding.workflow_id)?;
    let transaction = paths::project_workflow_transaction_file(&binding.workflow_id);
    if transaction.exists() {
        return Err(AppError::blocked(
            "TUI workflow read-only view는 pending recovery transaction을 실행하지 않습니다.",
        ));
    }
    let pointer_path = paths::project_workflow_file(&binding.workflow_id);
    let pointer_body = read_regular_file_bounded(&pointer_path, 64 * 1024, "TUI workflow pointer")?;
    let pointer = parse_workflow_pointer(&pointer_path, &pointer_body)?;
    snapshot_domain::validate_read_only_pointer(binding, &pointer)?;
    let snapshot_path =
        paths::project_workflow_snapshot_file(&binding.workflow_id, binding.revision);
    let snapshot_body =
        read_regular_file_bounded(&snapshot_path, 512 * 1024, "TUI workflow snapshot")?;
    if workflow_snapshot_schema(&snapshot_path, &snapshot_body)? != pointer.schema_version {
        return Err(AppError::blocked(
            "TUI workflow pointer/snapshot schema binding 불일치",
        ));
    }
    let workflow = parse_workflow_snapshot(&snapshot_path, &snapshot_body)?;
    snapshot_domain::validate_read_only_workflow(binding, identity, &workflow, ledger_events)?;
    Ok(workflow)
}

pub(crate) fn read_regular_file_bounded(
    path: &std::path::Path,
    max_bytes: u64,
    label: &str,
) -> Result<String, AppError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} metadata 실패: {err}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} regular-file/byte budget 불일치"
        )));
    }
    let mut file =
        File::open(path).map_err(|err| AppError::blocked(format!("{label} 열기 실패: {err}")))?;
    validate_open_read_identity(path, &file, label)?;
    let bytes = read_open_file_bounded(&mut file, max_bytes, label)?;
    validate_open_read_identity(path, &file, label)?;
    String::from_utf8(bytes).map_err(|_| AppError::blocked(format!("{label} UTF-8 불일치")))
}

pub(super) fn read_open_file_bounded(
    file: &mut File,
    max_bytes: u64,
    label: &str,
) -> Result<Vec<u8>, AppError> {
    let metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle metadata 실패: {err}")))?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} regular-file/byte budget 불일치"
        )));
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(usize::MAX)
            .min(usize::try_from(max_bytes).unwrap_or(usize::MAX)),
    );
    Read::by_ref(file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| AppError::blocked(format!("{label} 읽기 실패: {err}")))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} byte budget 초과; 증거를 보존했습니다."
        )));
    }
    let after = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 재검증 실패: {err}")))?;
    if !after.is_file() || after.len() > max_bytes {
        return Err(AppError::blocked(format!(
            "{label} read 중 byte budget 변경; 증거를 보존했습니다."
        )));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.dev() != file_metadata.dev()
        || path_metadata.ino() != file_metadata.ino()
    {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let same_file = windows_replace::path_refers_to_open_file(path, file)
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() || !same_file {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn validate_open_read_identity(
    path: &std::path::Path,
    file: &File,
    label: &str,
) -> Result<(), AppError> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|err| AppError::blocked(format!("{label} 경로 재검증 실패: {err}")))?;
    let file_metadata = file
        .metadata()
        .map_err(|err| AppError::blocked(format!("{label} handle 검증 실패: {err}")))?;
    if path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.len() != file_metadata.len()
    {
        return Err(AppError::blocked(format!(
            "{label} path/handle identity 불일치; 증거를 보존했습니다."
        )));
    }
    Ok(())
}

pub(super) fn tui_detail_value<'a>(details: &'a str, key: &str) -> Option<&'a str> {
    details.split_ascii_whitespace().find_map(|part| {
        let (candidate, value) = part.split_once('=')?;
        (candidate == key).then_some(value)
    })
}

fn with_validation_gap_writes_suppressed<T>(
    action: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    SUPPRESS_VALIDATION_GAP_WRITES.with(|flag| {
        let previous = flag.replace(true);
        let result = action();
        flag.set(previous);
        result
    })
}

pub(crate) fn current_state_lease_view_under_transition() -> Result<CurrentStateLeaseView, AppError>
{
    let path = paths::current_state_file();
    let body = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state lease 읽기 실패: {err}")))?;
    let snapshot = parse_current_state(&body, "current-state lease")?;
    if snapshot.schema_version == 1 {
        promote_current_state_v1()?;
        return current_state_lease_view_under_transition();
    }
    let current_ledger = ledger::validated_ledger_binding()?;
    if snapshot.ledger_binding != current_ledger {
        return snapshot_domain::validate_current_lease(&snapshot, &current_ledger, None);
    }
    let active_workflow = snapshot
        .active_workflow
        .as_ref()
        .map(|binding| load_workflow_under_transition(&binding.workflow_id))
        .transpose()?;
    snapshot_domain::validate_current_lease(&snapshot, &current_ledger, active_workflow.as_ref())
}

fn selection_observation_under_transition() -> Result<SelectionObservation, AppError> {
    let identity = ledger::validated_current_identity()?;
    let lease = current_state_lease_view_under_transition()?;
    let body = fs::read_to_string(paths::current_state_file())
        .map_err(|err| AppError::blocked(format!("selection current-state 읽기 실패: {err}")))?;
    let snapshot = parse_current_state(&body, "selection current-state")?;
    snapshot_domain::validate_snapshot_identity(&snapshot, &identity)?;
    let active = snapshot
        .active_workflow
        .as_ref()
        .map(|binding| load_workflow_under_transition(&binding.workflow_id))
        .transpose()?;
    Ok(SelectionObservation {
        project_id: identity.project_id,
        session_id: identity.session_id,
        current_revision: lease.revision,
        current_hash: lease.artifact_hash,
        active_workflow: active.map(|workflow| ObservedWorkflow {
            workflow_id: workflow.workflow_id,
            revision: workflow.revision,
            hash: workflow.artifact_hash,
        }),
    })
}

pub(crate) fn tui_lease_matches_workflow_under_transition(
    lease: &SelectionLease,
    workflow_id: &str,
) -> Result<bool, AppError> {
    let observation = selection_observation_under_transition()?;
    Ok(lease_matches_active_workflow(
        lease,
        workflow_id,
        &observation,
    ))
}

pub(crate) fn tui_lease_matches_terminal_selection_under_transition(
    lease: &SelectionLease,
    workflow_id: &str,
) -> Result<bool, AppError> {
    let observation = selection_observation_under_transition()?;
    Ok(lease_matches_terminal_selection(
        lease,
        workflow_id,
        &observation,
    ))
}

pub(super) fn promote_current_state_v1() -> Result<(), AppError> {
    let _transition = lease::RecoverableLease::acquire_with_wait(
        paths::current_state_transition_lock(),
        "current-state v1 promotion",
        Duration::from_secs(5),
    )?;
    let path = paths::current_state_file();
    let temporary = paths::current_state_v2_promotion_temp();
    let current_body = fs::read_to_string(&path)
        .map_err(|err| AppError::blocked(format!("current-state promotion 읽기 실패: {err}")))?;
    let current = parse_current_state(&current_body, "current-state promotion source")?;

    if current.schema_version == 2 {
        if temporary.exists() {
            let temp_body = fs::read_to_string(&temporary).map_err(|err| {
                AppError::blocked(format!("current-state promotion temp 읽기 실패: {err}"))
            })?;
            parse_current_state_v2(&temp_body, "current-state promotion redundant temp")?;
            if temp_body != current_body {
                return Err(AppError::blocked(
                    "current-state promotion 차단\n- 이유: v2 current-state와 promotion temp가 다릅니다.\n- 동작: 둘 다 보존했습니다.",
                ));
            }
            fs::remove_file(&temporary).map_err(|err| {
                AppError::runtime(format!("redundant promotion temp 제거 실패: {err}"))
            })?;
            sync_parent(&temporary)?;
        }
        return Ok(());
    }

    if current.schema_version != 1 {
        return Err(AppError::blocked(
            "current-state promotion 차단: exact schema v1이 아닙니다.",
        ));
    }
    let previous_artifact_hash = current
        .legacy_canonical_hash
        .clone()
        .ok_or_else(|| AppError::blocked("legacy current-state canonical hash 누락"))?;
    let active_workflow = current
        .active_workflow
        .as_ref()
        .map(|binding| load_workflow_under_transition(&binding.workflow_id))
        .transpose()?
        .map(|workflow| CurrentWorkflowBinding {
            workflow_id: workflow.workflow_id,
            revision: workflow.revision,
            artifact_hash: workflow.artifact_hash,
        });
    let mut promoted = CurrentStateSnapshot {
        schema_version: 2,
        revision: 1,
        previous_artifact_hash,
        project_id: current.project_id,
        project_root: current.project_root,
        session_id: current.session_id,
        active_workflow,
        parent_session_id: current.parent_session_id,
        branch_from_event_id: current.branch_from_event_id,
        compaction_boundary: current.compaction_boundary,
        resume_source: current.resume_source,
        // Schema v1 did not persist a ledger binding. Keep parsing/classification
        // independent of the ambient ledger; promotion binds the freshly
        // validated ledger when it constructs the schema-v2 image.
        ledger_binding: ledger::LedgerBinding {
            event_count: 0,
            event_id: None,
            event_hash: "root".to_string(),
        },
        artifact_hash: String::new(),
        legacy_canonical_hash: None,
    };
    promoted.artifact_hash = sha256_text(&render_current_state_v2_payload(&promoted));
    let prepared = render_current_state_v2(&promoted);

    if temporary.exists() {
        let temp_body = fs::read_to_string(&temporary).map_err(|err| {
            AppError::blocked(format!("current-state promotion temp 읽기 실패: {err}"))
        })?;
        let temp = parse_current_state_v2(&temp_body, "current-state promotion temp")?;
        if temp_body != prepared {
            if same_v1_promotion_except_ledger(&temp, &promoted)
                && temp.ledger_binding != promoted.ledger_binding
            {
                preserve_stale_promotion_temp(&temporary, &temp_body)?;
            } else {
                return Err(AppError::blocked(
                    "current-state promotion 차단\n- 이유: promotion temp가 현재 v1에서 파생되지 않았습니다.\n- 동작: current-state와 temp를 변경하지 않았습니다.",
                ));
            }
        }
    }

    if !temporary.exists() {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|err| {
            AppError::runtime(format!("current-state promotion temp 생성 실패: {err}"))
        })?;
        if let Ok(metadata) = fs::metadata(&path) {
            file.set_permissions(metadata.permissions())
                .map_err(|err| {
                    AppError::runtime(format!(
                        "current-state promotion permission 복사 실패: {err}"
                    ))
                })?;
        }
        file.write_all(prepared.as_bytes()).map_err(|err| {
            AppError::runtime(format!("current-state promotion temp write 실패: {err}"))
        })?;
        file.sync_all().map_err(|err| {
            AppError::runtime(format!("current-state promotion temp sync 실패: {err}"))
        })?;
        drop(file);
        promotion_fault("after-temp-sync")?;
    }

    replace_file(&temporary, &path).map_err(|err| {
        AppError::runtime(format!(
            "current-state promotion replace 실패: {} -> {} ({err})",
            temporary.display(),
            path.display()
        ))
    })?;
    promotion_fault("after-rename")?;
    sync_parent(&path)?;
    promotion_fault("after-parent-sync")?;

    let installed = fs::read_to_string(&path).map_err(|err| {
        AppError::blocked(format!("promoted current-state 재검증 읽기 실패: {err}"))
    })?;
    if installed != prepared {
        return Err(AppError::blocked(
            "current-state promotion 재검증 차단: 설치된 bytes 불일치",
        ));
    }
    let installed = parse_current_state_v2(&installed, "promoted current-state")?;
    if installed != promoted {
        return Err(AppError::blocked(
            "current-state promotion 재검증 차단: 설치된 binding 불일치",
        ));
    }
    Ok(())
}

fn same_v1_promotion_except_ledger(
    left: &CurrentStateSnapshot,
    right: &CurrentStateSnapshot,
) -> bool {
    left.schema_version == 2
        && left.revision == 1
        && left.previous_artifact_hash == right.previous_artifact_hash
        && left.project_id == right.project_id
        && left.project_root == right.project_root
        && left.session_id == right.session_id
        && left.active_workflow == right.active_workflow
        && left.parent_session_id == right.parent_session_id
        && left.branch_from_event_id == right.branch_from_event_id
        && left.compaction_boundary == right.compaction_boundary
        && left.resume_source == right.resume_source
}

fn preserve_stale_promotion_temp(path: &std::path::Path, bytes: &str) -> Result<(), AppError> {
    let diagnostic = path.with_file_name(format!(
        "current-state.json.v2-promote.tmp.stale-{}.diagnostic",
        sha256_text(bytes)
    ));
    if diagnostic.exists() {
        let existing = fs::read_to_string(&diagnostic)
            .map_err(|err| AppError::blocked(format!("promotion diagnostic 읽기 실패: {err}")))?;
        if existing != bytes {
            return Err(AppError::blocked(
                "current-state promotion diagnostic hash 충돌로 차단",
            ));
        }
        fs::remove_file(path)
            .map_err(|err| AppError::runtime(format!("stale promotion temp 제거 실패: {err}")))?;
    } else {
        fs::rename(path, &diagnostic).map_err(|err| {
            AppError::runtime(format!("stale promotion temp 보존 이동 실패: {err}"))
        })?;
    }
    sync_parent(&diagnostic)
}

fn parse_current_state_v1(
    body: &str,
    value: strict_json::Value,
    context: &str,
) -> Result<CurrentStateSnapshot, AppError> {
    let object = strict_json::parse_object(body, CURRENT_STATE_V1_KEYS, context)?;
    require_exact_key_set(&object, CURRENT_STATE_V1_KEYS, context)?;
    validate_terminal_states(object.get("terminal_states"), context)?;
    let active_workflow = match object.get("active_workflow") {
        Some(strict_json::Value::Null) => None,
        Some(strict_json::Value::String(workflow_id)) => {
            validate_current_id(workflow_id, "workflow_id", context)?;
            Some(CurrentWorkflowBinding {
                workflow_id: workflow_id.clone(),
                revision: 0,
                artifact_hash: String::new(),
            })
        }
        _ => return Err(current_state_field_error(context, "active_workflow")),
    };
    let project_id = strict_json::string(&object, "project_id", context)?;
    let session_id = strict_json::string(&object, "session_id", context)?;
    validate_current_id(&project_id, "project_id", context)?;
    validate_current_id(&session_id, "session_id", context)?;
    let canonical = strict_json::render_compact(&value);
    Ok(CurrentStateSnapshot {
        schema_version: 1,
        revision: 0,
        previous_artifact_hash: String::new(),
        project_id,
        project_root: strict_json::string(&object, "project_root", context)?,
        session_id,
        active_workflow,
        parent_session_id: optional_string(&object, "parent_session_id", context)?,
        branch_from_event_id: optional_string(&object, "branch_from_event_id", context)?,
        compaction_boundary: optional_string(&object, "compaction_boundary", context)?,
        resume_source: optional_string(&object, "resume_source", context)?,
        ledger_binding: ledger::validated_ledger_binding()?,
        artifact_hash: String::new(),
        legacy_canonical_hash: Some(sha256_text(&canonical)),
    })
}

pub(super) fn parse_current_state_v2(
    body: &str,
    context: &str,
) -> Result<CurrentStateSnapshot, AppError> {
    let canonical = strict_json::parse_canonical_object(body, CURRENT_STATE_V2_KEYS, context)?;
    if strict_json::canonical_u64(&canonical, "schema_version", context)? != 2 {
        return Err(current_state_field_error(context, "schema_version"));
    }
    let canonical_revision = strict_json::canonical_u64(&canonical, "revision", context)?;
    let object = strict_json::parse_object_exact_order(body, CURRENT_STATE_V2_KEYS, context)?;
    let revision = strict_json::number(&object, "revision", context)?;
    if revision == 0 || revision != canonical_revision {
        return Err(current_state_field_error(context, "revision"));
    }
    let previous_artifact_hash = strict_json::string(&object, "previous_artifact_hash", context)?;
    if previous_artifact_hash != "none" && !is_sha256(&previous_artifact_hash) {
        return Err(current_state_field_error(context, "previous_artifact_hash"));
    }
    let project_id = strict_json::string(&object, "project_id", context)?;
    let session_id = strict_json::string(&object, "session_id", context)?;
    validate_current_id(&project_id, "project_id", context)?;
    validate_current_id(&session_id, "session_id", context)?;
    let active_workflow = parse_current_workflow(object.get("active_workflow"), context)?;
    validate_terminal_states(object.get("terminal_states"), context)?;
    let ledger_binding = parse_current_ledger_binding(object.get("ledger_binding"), context)?;
    let artifact_hash = strict_json::string(&object, "artifact_hash", context)?;
    if !is_sha256(&artifact_hash) {
        return Err(current_state_field_error(context, "artifact_hash"));
    }
    let snapshot = CurrentStateSnapshot {
        schema_version: 2,
        revision,
        previous_artifact_hash,
        project_id,
        project_root: strict_json::string(&object, "project_root", context)?,
        session_id,
        active_workflow,
        parent_session_id: optional_string(&object, "parent_session_id", context)?,
        branch_from_event_id: optional_string(&object, "branch_from_event_id", context)?,
        compaction_boundary: optional_string(&object, "compaction_boundary", context)?,
        resume_source: optional_string(&object, "resume_source", context)?,
        ledger_binding,
        artifact_hash,
        legacy_canonical_hash: None,
    };
    let payload = render_current_state_v2_payload(&snapshot);
    if sha256_text(&payload) != snapshot.artifact_hash || render_current_state_v2(&snapshot) != body
    {
        return Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: artifact hash 또는 canonical re-render 불일치"
        )));
    }
    Ok(snapshot)
}

pub(super) fn render_current_state_v2(snapshot: &CurrentStateSnapshot) -> String {
    let payload = render_current_state_v2_payload(snapshot);
    format!(
        "{},\"artifact_hash\":\"{}\"}}",
        payload
            .strip_suffix('}')
            .expect("current-state payload object"),
        snapshot.artifact_hash
    )
}

pub(super) fn render_current_state_v2_payload(snapshot: &CurrentStateSnapshot) -> String {
    let active_workflow = snapshot
        .active_workflow
        .as_ref()
        .map(|workflow| {
            format!(
                "{{\"workflow_id\":\"{}\",\"revision\":{},\"artifact_hash\":\"{}\"}}",
                ledger::json_string(&workflow.workflow_id),
                workflow.revision,
                workflow.artifact_hash
            )
        })
        .unwrap_or_else(|| "null".to_string());
    let event_id = snapshot
        .ledger_binding
        .event_id
        .as_ref()
        .map(|value| format!("\"{}\"", ledger::json_string(value)))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"schema_version\":2,\"revision\":{},\"previous_artifact_hash\":\"{}\",\"project_id\":\"{}\",\"project_root\":\"{}\",\"session_id\":\"{}\",\"active_workflow\":{},\"parent_session_id\":{},\"branch_from_event_id\":{},\"compaction_boundary\":{},\"resume_source\":{},\"terminal_states\":[\"complete\",\"failed\",\"cancelled\"],\"ledger_binding\":{{\"event_count\":{},\"event_id\":{},\"event_hash\":\"{}\"}}}}",
        snapshot.revision,
        snapshot.previous_artifact_hash,
        ledger::json_string(&snapshot.project_id),
        ledger::json_string(&snapshot.project_root),
        ledger::json_string(&snapshot.session_id),
        active_workflow,
        render_optional_string(snapshot.parent_session_id.as_deref()),
        render_optional_string(snapshot.branch_from_event_id.as_deref()),
        render_optional_string(snapshot.compaction_boundary.as_deref()),
        render_optional_string(snapshot.resume_source.as_deref()),
        snapshot.ledger_binding.event_count,
        event_id,
        snapshot.ledger_binding.event_hash,
    )
}

fn render_optional_string(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", ledger::json_string(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn parse_current_workflow(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<Option<CurrentWorkflowBinding>, AppError> {
    match value {
        Some(strict_json::Value::Null) => Ok(None),
        Some(strict_json::Value::Object(object)) => {
            let expected = ["workflow_id", "revision", "artifact_hash"];
            require_exact_key_order(object, &expected, context)?;
            let workflow_id = strict_json::string(object, "workflow_id", context)?;
            validate_current_id(&workflow_id, "workflow_id", context)?;
            let revision = strict_json::number(object, "revision", context)?;
            let artifact_hash = strict_json::string(object, "artifact_hash", context)?;
            if revision == 0 || !is_sha256(&artifact_hash) {
                return Err(current_state_field_error(context, "active_workflow"));
            }
            Ok(Some(CurrentWorkflowBinding {
                workflow_id,
                revision,
                artifact_hash,
            }))
        }
        _ => Err(current_state_field_error(context, "active_workflow")),
    }
}

fn parse_current_ledger_binding(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<ledger::LedgerBinding, AppError> {
    let Some(strict_json::Value::Object(object)) = value else {
        return Err(current_state_field_error(context, "ledger_binding"));
    };
    let expected = ["event_count", "event_id", "event_hash"];
    require_exact_key_order(object, &expected, context)?;
    let event_count = strict_json::number(object, "event_count", context)?;
    let event_id = optional_string(object, "event_id", context)?;
    if let Some(event_id) = event_id.as_deref() {
        validate_current_id(event_id, "event_id", context)?;
    }
    let event_hash = strict_json::string(object, "event_hash", context)?;
    if (event_count == 0 && (event_id.is_some() || event_hash != "root"))
        || (event_count > 0 && (event_id.is_none() || !is_sha256(&event_hash)))
    {
        return Err(current_state_field_error(context, "ledger_binding"));
    }
    Ok(ledger::LedgerBinding {
        event_count,
        event_id,
        event_hash,
    })
}

fn optional_string(
    object: &strict_json::Object,
    key: &str,
    context: &str,
) -> Result<Option<String>, AppError> {
    match object.get(key) {
        Some(strict_json::Value::Null) => Ok(None),
        Some(strict_json::Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(current_state_field_error(context, key)),
    }
}

fn validate_terminal_states(
    value: Option<&strict_json::Value>,
    context: &str,
) -> Result<(), AppError> {
    let Some(strict_json::Value::Array(values)) = value else {
        return Err(current_state_field_error(context, "terminal_states"));
    };
    let actual = values
        .iter()
        .map(|value| match value {
            strict_json::Value::String(value) => Some(value.as_str()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>();
    if actual.as_deref() == Some(["complete", "failed", "cancelled"].as_slice()) {
        Ok(())
    } else {
        Err(current_state_field_error(context, "terminal_states"))
    }
}

fn require_exact_key_set(
    object: &strict_json::Object,
    keys: &[&str],
    context: &str,
) -> Result<(), AppError> {
    if object.len() == keys.len() && keys.iter().all(|key| object.contains_key(key)) {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: exact key set 불일치"
        )))
    }
}

fn require_exact_key_order(
    object: &strict_json::Object,
    keys: &[&str],
    context: &str,
) -> Result<(), AppError> {
    let actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    if actual == keys {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "{context} 차단\n- 이유: exact nested key order 불일치"
        )))
    }
}

fn validate_current_id(value: &str, field: &str, context: &str) -> Result<(), AppError> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if valid {
        Ok(())
    } else {
        Err(current_state_field_error(context, field))
    }
}

fn current_state_field_error(context: &str, field: &str) -> AppError {
    AppError::blocked(format!(
        "{context} 차단\n- 이유: invalid current-state field\n- field: {field}"
    ))
}

pub(super) fn read_current_state_summary() -> Result<String, AppError> {
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

    let identity = ledger::fresh_identity();
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

pub(super) fn current_state_status(
    identity: &RuntimeIdentity,
) -> Result<CurrentStateStatus, AppError> {
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

pub(super) fn classify_current_state(
    contents: &str,
    identity: &RuntimeIdentity,
) -> CurrentStateStatus {
    let Ok(snapshot) = parse_current_state(contents, "current-state classification") else {
        return CurrentStateStatus::Corrupt;
    };
    if snapshot.project_id != identity.project_id || snapshot.project_root != identity.project_root
    {
        return CurrentStateStatus::StaleProject;
    }
    match snapshot.active_workflow {
        None => CurrentStateStatus::CleanNoActiveWorkflow,
        Some(_) => CurrentStateStatus::CleanActiveWorkflow,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CurrentStateStatus {
    Missing,
    Corrupt,
    StaleProject,
    CleanNoActiveWorkflow,
    CleanActiveWorkflow,
}
