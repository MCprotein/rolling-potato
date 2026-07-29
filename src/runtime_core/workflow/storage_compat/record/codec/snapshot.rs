use std::path::Path;

use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

use super::super::{sha256_text, WorkflowRecord};
use super::payload_codec::{payload, payload_v2, payload_v3};
use super::versions::{
    LEGACY_WORKFLOW_SCHEMA_VERSION, PREVIOUS_WORKFLOW_SCHEMA_VERSION, WORKFLOW_SCHEMA_VERSION,
    WORKFLOW_V2_KEYS, WORKFLOW_V3_KEYS, WORKFLOW_V4_KEYS,
};

pub(crate) fn snapshot_schema(
    path: &Path,
    body: &str,
    corrupt: fn(&Path) -> AppError,
) -> Result<u64, AppError> {
    let context = path.display().to_string();
    let object =
        strict_json::parse_object(body, WORKFLOW_V4_KEYS, &context).map_err(|_| corrupt(path))?;
    let schema =
        strict_json::number(&object, "schema_version", &context).map_err(|_| corrupt(path))?;
    let (keys, artifact_version) = match schema {
        LEGACY_WORKFLOW_SCHEMA_VERSION => (WORKFLOW_V2_KEYS, "workflow-v2"),
        PREVIOUS_WORKFLOW_SCHEMA_VERSION => (WORKFLOW_V3_KEYS, "workflow-v3"),
        WORKFLOW_SCHEMA_VERSION => (WORKFLOW_V4_KEYS, "workflow-v4"),
        _ => return Err(corrupt(path)),
    };
    if object.len() != keys.len()
        || keys.iter().any(|key| !object.contains_key(key))
        || strict_json::string(&object, "artifact_version", &context).map_err(|_| corrupt(path))?
            != artifact_version
    {
        return Err(corrupt(path));
    }
    Ok(schema)
}

pub(crate) fn parse_snapshot(
    path: &Path,
    body: &str,
    corrupt: fn(&Path) -> AppError,
) -> Result<WorkflowRecord, AppError> {
    let schema = snapshot_schema(path, body, corrupt)?;
    let keys = match schema {
        LEGACY_WORKFLOW_SCHEMA_VERSION => WORKFLOW_V2_KEYS,
        PREVIOUS_WORKFLOW_SCHEMA_VERSION => WORKFLOW_V3_KEYS,
        WORKFLOW_SCHEMA_VERSION => WORKFLOW_V4_KEYS,
        _ => return Err(corrupt(path)),
    };
    let context = path.display().to_string();
    let object = strict_json::parse_object(body, keys, &context).map_err(|_| corrupt(path))?;
    let text = |key| strict_json::string(&object, key, &context).map_err(|_| corrupt(path));
    let mut record = WorkflowRecord {
        workflow_id: text("workflow_id")?,
        revision: strict_json::number(&object, "revision", &context).map_err(|_| corrupt(path))?,
        previous_hash: text("previous_hash")?,
        artifact_hash: text("artifact_hash")?,
        project_id: text("project_id")?,
        session_id: text("session_id")?,
        phase: text("phase")?,
        request_hash: text("request_hash")?,
        workflow_kind: text("workflow_kind")?,
        active_skill_id: if schema == WORKFLOW_SCHEMA_VERSION {
            text("active_skill_id")?
        } else {
            String::new()
        },
        skill_invocation: if schema == WORKFLOW_SCHEMA_VERSION {
            text("skill_invocation")?
        } else {
            String::new()
        },
        skill_state: if schema == WORKFLOW_SCHEMA_VERSION {
            text("skill_state")?
        } else {
            String::new()
        },
        skill_completed_hooks: if schema == WORKFLOW_SCHEMA_VERSION {
            text("skill_completed_hooks")?
        } else {
            String::new()
        },
        skill_evidence: if schema == WORKFLOW_SCHEMA_VERSION {
            text("skill_evidence")?
        } else {
            String::new()
        },
        skill_stop_criteria: if schema == WORKFLOW_SCHEMA_VERSION {
            text("skill_stop_criteria")?
        } else {
            String::new()
        },
        action_id: text("action_id")?,
        action_kind: text("action_kind")?,
        action_status: text("action_status")?,
        result_summary: text("result_summary")?,
        source_path: text("source_path")?,
        source_hash: text("source_hash")?,
        find_text: text("find_text")?,
        replace_text: text("replace_text")?,
        proposal_id: text("proposal_id")?,
        proposal_hash: text("proposal_hash")?,
        approval_credential_hash: text("approval_credential_hash")?,
        before_hash: text("before_hash")?,
        after_hash: text("after_hash")?,
        verification_plan: text("verification_plan")?,
        approval_state: text("approval_state")?,
        verification_credential_hash: if schema >= PREVIOUS_WORKFLOW_SCHEMA_VERSION {
            text("verification_credential_hash")?
        } else {
            String::new()
        },
        verification_approval_state: if schema >= PREVIOUS_WORKFLOW_SCHEMA_VERSION {
            text("verification_approval_state")?
        } else {
            "not-issued".to_string()
        },
        evidence_id: text("evidence_id")?,
        evidence_hash: text("evidence_hash")?,
        failure_reason: text("failure_reason")?,
    };
    let payload = match schema {
        LEGACY_WORKFLOW_SCHEMA_VERSION => payload_v2(&record),
        PREVIOUS_WORKFLOW_SCHEMA_VERSION => payload_v3(&record),
        WORKFLOW_SCHEMA_VERSION => payload(&record),
        _ => return Err(corrupt(path)),
    };
    if record.artifact_hash != sha256_text(&payload) {
        return Err(corrupt(path));
    }
    if schema == LEGACY_WORKFLOW_SCHEMA_VERSION {
        map_legacy_approval_state(&mut record);
    }
    Ok(record)
}

fn map_legacy_approval_state(record: &mut WorkflowRecord) {
    match record.phase.as_str() {
        "verification-started" if !record.proposal_id.is_empty() => {
            record.approval_state = "applied".to_string();
            record.verification_approval_state = "approved".to_string();
        }
        "verified" | "complete"
            if !record.proposal_id.is_empty()
                && !record.evidence_id.is_empty()
                && !record.source_path.is_empty()
                && !record.after_hash.is_empty() =>
        {
            record.approval_state = "applied".to_string();
            record.verification_approval_state = "approved".to_string();
        }
        "failed" if !record.proposal_id.is_empty() && !record.evidence_id.is_empty() => {
            record.approval_state = "applied".to_string();
            record.verification_approval_state = "approved".to_string();
        }
        "cancelled" if !record.proposal_id.is_empty() => {
            record.verification_approval_state = "cancelled".to_string();
        }
        _ => {}
    }
}
