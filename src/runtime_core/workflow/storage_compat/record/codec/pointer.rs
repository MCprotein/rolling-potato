use std::path::Path;

use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;

use super::super::WorkflowRecord;
use super::versions::{
    LEGACY_WORKFLOW_SCHEMA_VERSION, PREVIOUS_WORKFLOW_SCHEMA_VERSION, WORKFLOW_SCHEMA_VERSION,
};

pub(crate) fn render_pointer(
    record: &WorkflowRecord,
    schema_version: u64,
) -> Result<String, AppError> {
    let artifact_version = match schema_version {
        LEGACY_WORKFLOW_SCHEMA_VERSION => "workflow-commit-v2",
        PREVIOUS_WORKFLOW_SCHEMA_VERSION => "workflow-commit-v3",
        WORKFLOW_SCHEMA_VERSION => "workflow-commit-v4",
        _ => {
            return Err(AppError::blocked(
                "workflow pointer schema 지원 범위 밖입니다.",
            ))
        }
    };
    let body = format!(
        "{{\n  \"schema_version\": {schema_version},\n  \"artifact_version\": \"{artifact_version}\",\n  \"workflow_id\": \"{}\",\n  \"committed_revision\": {},\n  \"artifact_hash\": \"{}\"\n}}\n",
        strict_json::escape_string_content(&record.workflow_id),
        record.revision,
        record.artifact_hash
    );
    Ok(body)
}

#[derive(Debug)]
pub(crate) struct WorkflowPointer {
    pub(crate) schema_version: u64,
    pub(crate) workflow_id: String,
    pub(crate) committed_revision: u64,
    pub(crate) artifact_hash: String,
}

pub(crate) fn parse_pointer(
    path: &Path,
    body: &str,
    corrupt: fn(&Path) -> AppError,
) -> Result<WorkflowPointer, AppError> {
    const KEYS: &[&str] = &[
        "schema_version",
        "artifact_version",
        "workflow_id",
        "committed_revision",
        "artifact_hash",
    ];
    let context = path.display().to_string();
    let object = strict_json::parse_object(body, KEYS, &context).map_err(|_| corrupt(path))?;
    if object.len() != KEYS.len() || KEYS.iter().any(|key| !object.contains_key(key)) {
        return Err(corrupt(path));
    }
    let schema_version =
        strict_json::number(&object, "schema_version", &context).map_err(|_| corrupt(path))?;
    let artifact_version =
        strict_json::string(&object, "artifact_version", &context).map_err(|_| corrupt(path))?;
    let expected_artifact_version = match schema_version {
        LEGACY_WORKFLOW_SCHEMA_VERSION => "workflow-commit-v2",
        PREVIOUS_WORKFLOW_SCHEMA_VERSION => "workflow-commit-v3",
        WORKFLOW_SCHEMA_VERSION => "workflow-commit-v4",
        _ => return Err(corrupt(path)),
    };
    if artifact_version != expected_artifact_version {
        return Err(corrupt(path));
    }
    Ok(WorkflowPointer {
        schema_version,
        workflow_id: strict_json::string(&object, "workflow_id", &context)
            .map_err(|_| corrupt(path))?,
        committed_revision: strict_json::number(&object, "committed_revision", &context)
            .map_err(|_| corrupt(path))?,
        artifact_hash: strict_json::string(&object, "artifact_hash", &context)
            .map_err(|_| corrupt(path))?,
    })
}
