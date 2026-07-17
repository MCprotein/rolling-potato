use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adapters::filesystem::{layout as paths, lease};
use crate::app::workflow_adapter::{ledger, state};
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::subagent as subagent_policy;
use crate::runtime_core::collaboration::subagent::{
    immutable_binding_changed, parse_record, render_payload, render_record, validate_record,
    validate_subagent_id, NewRecordBinding, SubagentRecordV1, SubagentStatus, ValidatedLaunch,
    MAX_RECORD_REVISIONS,
};

const MAX_SUBAGENT_RECORDS: usize = 256;
static SUBAGENT_ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(
        &current_path,
        body.as_bytes(),
    )?;
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

pub(super) fn latest_active_parent_record() -> Result<SubagentRecordV1, AppError> {
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
    crate::adapters::filesystem::atomic_write::atomic_replace_bytes(&path, body.as_bytes())
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
