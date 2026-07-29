use super::*;
use std::fs::OpenOptions;
use std::io::Write;

use crate::adapters::filesystem::atomic_write::{replace_file, sync_parent};

pub(in crate::app::workflow_adapter::state) fn promote_current_state_v1() -> Result<(), AppError> {
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
