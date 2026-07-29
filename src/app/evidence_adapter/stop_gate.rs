use std::fs;

use crate::adapters::filesystem::layout as paths;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::runtime_core::knowledge::evidence::{validate_stop_inputs, StopGateInputs};

pub fn evaluate_patch_stop_gate(workflow: &state::WorkflowRecord) -> Result<(), AppError> {
    validate_patch_stop_gate_inner(workflow, true)
}

pub fn validate_patch_stop_gate(workflow: &state::WorkflowRecord) -> Result<(), AppError> {
    validate_patch_stop_gate_inner(workflow, false)
}

fn validate_patch_stop_gate_inner(
    workflow: &state::WorkflowRecord,
    record_event: bool,
) -> Result<(), AppError> {
    let path = paths::project_evidence_dir().join(format!("{}.json", workflow.evidence_id));
    let body = fs::read_to_string(&path)
        .map_err(|_| stop_gate_error(workflow, "verification evidence missing", record_event))?;
    const KEYS: &[&str] = &[
        "schema_version",
        "evidence_id",
        "artifact_hash",
        "workflow_id",
        "proposal_id",
        "action_id",
        "command_hash",
        "passed",
        "exit_code",
        "source_hash",
        "stdout_hash",
        "stderr_hash",
    ];
    let object = strict_json::parse_object(&body, KEYS, "verification evidence")
        .map_err(|_| stop_gate_error(workflow, "malformed verification evidence", record_event))?;
    if strict_json::number(&object, "schema_version", "verification evidence")
        .map_err(|_| stop_gate_error(workflow, "malformed verification evidence", record_event))?
        != 1
    {
        return Err(stop_gate_error(
            workflow,
            "verification evidence schema version mismatch",
            record_event,
        ));
    }
    let field = |key| {
        strict_json::string(&object, key, "verification evidence")
            .map_err(|_| stop_gate_error(workflow, "malformed verification evidence", record_event))
    };
    let evidence_id = field("evidence_id")?;
    let body_artifact_hash = field("artifact_hash")?;
    let evidence_workflow = field("workflow_id")?;
    let evidence_proposal = field("proposal_id")?;
    let evidence_action = field("action_id")?;
    let command_hash = field("command_hash")?;
    let exit_code = field("exit_code")?;
    let source_hash = field("source_hash")?;
    let stdout_hash = field("stdout_hash")?;
    let stderr_hash = field("stderr_hash")?;
    let passed = strict_json::boolean(&object, "passed", "verification evidence")
        .map_err(|_| stop_gate_error(workflow, "malformed verification evidence", record_event))?;
    let payload = format!(
        "workflow_id={}\nproposal_id={}\naction_id={}\ncommand_hash={}\npassed={}\nexit_code={}\nsource_hash={}\nstdout_hash={}\nstderr_hash={}\n",
        evidence_workflow,
        evidence_proposal,
        evidence_action,
        command_hash,
        passed,
        exit_code,
        source_hash,
        stdout_hash,
        stderr_hash
    );
    let recomputed_hash = state::sha256_text(&payload);
    let source =
        fs::read_to_string(paths::project_root().join(&workflow.source_path)).map_err(|_| {
            stop_gate_error(workflow, "authoritative source reread failed", record_event)
        })?;
    let expected_command_hash = state::sha256_text(&workflow.verification_plan);
    let authoritative_source_hash = state::sha256_text(&source);
    if !validate_stop_inputs(&StopGateInputs {
        phase: &workflow.phase,
        approval_state: &workflow.approval_state,
        verification_approval_state: &workflow.verification_approval_state,
        expected_workflow_id: &workflow.workflow_id,
        expected_proposal_id: &workflow.proposal_id,
        expected_action_id: &workflow.action_id,
        expected_evidence_id: &workflow.evidence_id,
        expected_evidence_hash: &workflow.evidence_hash,
        expected_command_hash: &expected_command_hash,
        expected_source_hash: &workflow.after_hash,
        evidence_workflow_id: &evidence_workflow,
        evidence_proposal_id: &evidence_proposal,
        evidence_action_id: &evidence_action,
        evidence_id: &evidence_id,
        body_artifact_hash: &body_artifact_hash,
        recomputed_artifact_hash: &recomputed_hash,
        command_hash: &command_hash,
        source_hash: &source_hash,
        authoritative_source_hash: &authoritative_source_hash,
        passed,
    }) {
        return Err(stop_gate_error(
            workflow,
            "missing or stale applied/verification evidence",
            record_event,
        ));
    }
    if record_event {
        state::record_event(
            "workflow.stop_gate.passed",
            "workflow stop gate passed",
            &format!(
                "workflow_id={} proposal_id={} evidence_id={} applied_hash={} unresolved_approval=false",
                workflow.workflow_id, workflow.proposal_id, workflow.evidence_id, workflow.after_hash
            ),
        )?;
    }
    Ok(())
}

fn stop_gate_error(workflow: &state::WorkflowRecord, reason: &str, record_event: bool) -> AppError {
    let persistence = if record_event {
        state::record_event(
            "workflow.stop_gate.failed",
            "workflow stop gate failed",
            &format!(
                "workflow_id={} reason={}",
                workflow.workflow_id,
                reason.replace(' ', "-")
            ),
        )
        .err()
        .map(|err| format!("\n- stop-gate failure event 저장 실패: {}", err.message))
        .unwrap_or_default()
    } else {
        String::new()
    };
    AppError::blocked(format!(
        "workflow stop gate 차단\n- workflow id: {}\n- 이유: {}\n- 동작: 성공 보고를 생성하지 않습니다.{}",
        workflow.workflow_id, reason, persistence
    ))
}
