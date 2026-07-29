//! Session-resume domain validation.

use crate::foundation::error::AppError;
use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;

pub(crate) fn validate_session_resume_target(
    session_id: &str,
    canonical_session: bool,
    projected_session: bool,
    active_workflow: Option<&WorkflowRecord>,
) -> Result<Option<String>, AppError> {
    if !canonical_session {
        return Err(AppError::blocked(format!(
            "session resume 차단\n- session id: {}\n- 이유: canonical runtime ledger에서 현재 project의 session을 찾지 못했습니다.\n- 확인: `rpotato session list`",
            session_id
        )));
    }
    if !projected_session {
        return Err(AppError::blocked(format!(
            "session resume 차단\n- session id: {}\n- 이유: canonical ledger에는 존재하지만 SQLite projection 재생성 후 session을 찾지 못했습니다.\n- 확인: `rpotato state status`",
            session_id
        )));
    }
    if let Some(workflow) = active_workflow {
        if workflow.session_id != session_id {
            return Err(AppError::blocked(format!(
                "session resume 차단\n- session id: {}\n- 이유: 다른 session이 소유한 non-terminal workflow가 있습니다.\n- active workflow: {}\n- owner session: {}\n- 동작: current-state를 변경하지 않았습니다.",
                session_id, workflow.workflow_id, workflow.session_id
            )));
        }
    }
    Ok(active_workflow.map(|workflow| workflow.workflow_id.clone()))
}
