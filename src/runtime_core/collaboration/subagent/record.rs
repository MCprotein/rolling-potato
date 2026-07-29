//! Durable subagent record creation and lifecycle transitions.

use crate::foundation::error::AppError;

use super::record_validation::validate_record;
use super::types::{SubagentRecordV1, SubagentStatus, ValidatedLaunch};

pub(crate) struct NewRecordBinding<'a> {
    pub subagent_id: String,
    pub project_id: &'a str,
    pub session_id: &'a str,
    pub parent_workflow_id: &'a str,
    pub parent_revision: u64,
    pub parent_artifact_hash: &'a str,
    pub created_at_ms: u128,
}

impl SubagentRecordV1 {
    pub(crate) fn transition_to_at(
        &mut self,
        next: SubagentStatus,
        failure_code: Option<&str>,
        timestamp: u128,
    ) -> Result<(), AppError> {
        if !self.status.permits(next) {
            return Err(AppError::blocked(format!(
                "subagent 상태 전이 차단\n- current: {}\n- next: {}",
                self.status.as_str(),
                next.as_str()
            )));
        }
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

pub(crate) fn create_record_at(
    binding: NewRecordBinding<'_>,
    launch: ValidatedLaunch,
) -> Result<SubagentRecordV1, AppError> {
    let record = SubagentRecordV1 {
        subagent_id: binding.subagent_id,
        revision: 0,
        previous_hash: String::new(),
        artifact_hash: String::new(),
        project_id: binding.project_id.to_string(),
        session_id: binding.session_id.to_string(),
        parent_workflow_id: binding.parent_workflow_id.to_string(),
        parent_revision: binding.parent_revision,
        parent_artifact_hash: binding.parent_artifact_hash.to_string(),
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
        created_at_ms: binding.created_at_ms,
        started_at_ms: 0,
        finished_at_ms: 0,
    };
    validate_record(&record, false)?;
    Ok(record)
}
