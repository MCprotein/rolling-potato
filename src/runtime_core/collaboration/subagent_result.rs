//! Canonical subagent result shape and patch proposal policy.

mod evidence;

use super::subagent::{normalize_relative_path, SubagentRecordV1, SubagentRole};
use crate::foundation::error::AppError;
use crate::foundation::serialization as strict_json;
use crate::foundation::serialization::{CanonicalObject, CanonicalValue};
use std::collections::BTreeSet;

pub(crate) use evidence::{
    evidence_id, evidence_source_bindings, has_artifact_id, installable_evidence_body,
    render_evidence_payload_v2, verify_evidence_artifact,
};

pub const MAX_RESULT_BYTES: usize = 65_536;
const MAX_SUMMARY_BYTES: usize = 4_096;
const MAX_ITEM_BYTES: usize = 2_048;
const MAX_ITEMS: usize = 16;
pub(crate) const MAX_PATCH_TEXT_BYTES: usize = 32_768;
const RESULT_KEYS: &[&str] = &[
    "schema_version",
    "subagent_id",
    "parent_workflow_id",
    "role",
    "status",
    "summary",
    "findings",
    "patch_proposal",
    "evidence_refs",
    "validation_gaps",
    "suggested_next_action",
];
const PATCH_KEYS: &[&str] = &[
    "target_path",
    "source_hash",
    "find_text",
    "replacement_text",
];
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentPatchProposalV1 {
    pub target_path: String,
    pub source_hash: String,
    pub find_text: String,
    pub replacement_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentResultV1 {
    pub subagent_id: String,
    pub parent_workflow_id: String,
    pub role: String,
    pub status: String,
    pub summary: String,
    pub findings: Vec<String>,
    pub patch_proposal: Option<SubagentPatchProposalV1>,
    pub evidence_refs: Vec<String>,
    pub validation_gaps: Vec<String>,
    pub suggested_next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvidenceSourceBinding {
    pub path: String,
    pub stable_ref: String,
    pub fingerprint: String,
}

pub(crate) struct SourcePointerBinding<'a> {
    pub path: &'a str,
    pub stable_ref: &'a str,
    pub fingerprint: &'a str,
}

pub(crate) struct ResultBinding<'a> {
    pub subagent_id: &'a str,
    pub parent_workflow_id: &'a str,
    pub role: SubagentRole,
}

pub(crate) struct PatchPolicyBinding<'a> {
    pub role: SubagentRole,
    pub declared_tools: &'a [String],
    pub read_paths: &'a [String],
    pub write_paths: &'a [String],
}

pub(crate) fn parse_result_shape(
    binding: &ResultBinding<'_>,
    body: &str,
) -> Result<SubagentResultV1, AppError> {
    if body.is_empty() || body.len() > MAX_RESULT_BYTES {
        return Err(AppError::blocked(format!(
            "subagent result byte 범위 오류: 1..={MAX_RESULT_BYTES}"
        )));
    }
    let object = strict_json::parse_canonical_object(body, RESULT_KEYS, "subagent result")?;
    if strict_json::canonical_u64(&object, "schema_version", "subagent result")? != 1 {
        return Err(AppError::blocked("subagent result schema version 불일치"));
    }
    let result = SubagentResultV1 {
        subagent_id: string(&object, "subagent_id")?,
        parent_workflow_id: string(&object, "parent_workflow_id")?,
        role: string(&object, "role")?,
        status: string(&object, "status")?,
        summary: string(&object, "summary")?,
        findings: string_array(&object, "findings")?,
        patch_proposal: patch(&object)?,
        evidence_refs: string_array(&object, "evidence_refs")?,
        validation_gaps: string_array(&object, "validation_gaps")?,
        suggested_next_action: string(&object, "suggested_next_action")?,
    };
    if result.subagent_id != binding.subagent_id
        || result.parent_workflow_id != binding.parent_workflow_id
        || result.role != binding.role.as_str()
        || result.status != "completed"
    {
        return Err(AppError::blocked(
            "subagent result identity/status binding 불일치",
        ));
    }
    validate_bounded_text(&result.summary, "summary", 1, MAX_SUMMARY_BYTES)?;
    validate_bounded_text(
        &result.suggested_next_action,
        "suggested_next_action",
        0,
        MAX_ITEM_BYTES,
    )?;
    validate_items(&result.findings, "findings", false)?;
    validate_items(&result.evidence_refs, "evidence_refs", true)?;
    validate_items(&result.validation_gaps, "validation_gaps", false)?;
    Ok(result)
}

pub(crate) fn validate_patch_policy(
    binding: &PatchPolicyBinding<'_>,
    patch: Option<&SubagentPatchProposalV1>,
) -> Result<(), AppError> {
    let Some(patch) = patch else {
        return Ok(());
    };
    if binding.role != SubagentRole::Executor
        || !binding
            .declared_tools
            .iter()
            .any(|tool| tool == "render_diff")
    {
        return Err(AppError::blocked(
            "executor/render_diff가 아닌 subagent patch proposal 차단",
        ));
    }
    let normalized = normalize_relative_path(&patch.target_path)?;
    if normalized != patch.target_path
        || !binding.read_paths.iter().any(|path| path == &normalized)
        || !binding.write_paths.iter().any(|owner| {
            normalized == *owner
                || normalized
                    .strip_prefix(owner)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        })
    {
        return Err(AppError::blocked(
            "subagent patch target declared read/write ownership 불일치",
        ));
    }
    validate_bounded_text(&patch.find_text, "patch.find_text", 1, MAX_PATCH_TEXT_BYTES)?;
    validate_bounded_text(
        &patch.replacement_text,
        "patch.replacement_text",
        0,
        MAX_PATCH_TEXT_BYTES,
    )?;
    if patch.find_text == patch.replacement_text {
        return Err(AppError::blocked(
            "subagent patch proposal은 실제 변경이어야 합니다.",
        ));
    }
    Ok(())
}

pub(crate) fn validate_context_binding(
    record: &SubagentRecordV1,
    result: &SubagentResultV1,
    sources: &[SourcePointerBinding<'_>],
) -> Result<(), AppError> {
    let allowed_evidence = sources
        .iter()
        .map(|pointer| pointer.stable_ref)
        .collect::<BTreeSet<_>>();
    if result
        .evidence_refs
        .iter()
        .any(|reference| !allowed_evidence.contains(reference.as_str()))
    {
        return Err(AppError::blocked(
            "subagent result evidence ref가 declared context binding 밖입니다.",
        ));
    }
    validate_patch_policy(
        &PatchPolicyBinding {
            role: record.role,
            declared_tools: &record.declared_tools,
            read_paths: &record.read_paths,
            write_paths: &record.write_paths,
        },
        result.patch_proposal.as_ref(),
    )?;
    let Some(patch) = result.patch_proposal.as_ref() else {
        return Ok(());
    };
    let Some(pointer) = sources
        .iter()
        .find(|pointer| pointer.path == patch.target_path)
    else {
        return Err(AppError::blocked("subagent patch source context 누락"));
    };
    if patch.source_hash != pointer.fingerprint {
        return Err(AppError::blocked("subagent patch source hash 불일치"));
    }
    Ok(())
}

fn patch(object: &CanonicalObject) -> Result<Option<SubagentPatchProposalV1>, AppError> {
    match object.get("patch_proposal") {
        Some(CanonicalValue::Null) => Ok(None),
        Some(CanonicalValue::Object(patch)) => {
            let actual = patch
                .entries
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>();
            if actual != PATCH_KEYS {
                return Err(AppError::blocked(
                    "subagent patch proposal exact key order 불일치",
                ));
            }
            Ok(Some(SubagentPatchProposalV1 {
                target_path: string(patch, "target_path")?,
                source_hash: string(patch, "source_hash")?,
                find_text: string(patch, "find_text")?,
                replacement_text: string(patch, "replacement_text")?,
            }))
        }
        _ => Err(AppError::blocked(
            "subagent result patch_proposal type 오류",
        )),
    }
}

fn string(object: &CanonicalObject, key: &str) -> Result<String, AppError> {
    match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "subagent result missing/wrong string: {key}"
        ))),
    }
}

fn string_array(object: &CanonicalObject, key: &str) -> Result<Vec<String>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get(key) else {
        return Err(AppError::blocked(format!(
            "subagent result missing/wrong array: {key}"
        )));
    };
    values
        .iter()
        .map(|value| match value {
            CanonicalValue::String(value) => Ok(value.clone()),
            _ => Err(AppError::blocked(format!(
                "subagent result array item type 오류: {key}"
            ))),
        })
        .collect()
}

fn validate_items(values: &[String], label: &str, required: bool) -> Result<(), AppError> {
    if values.len() > MAX_ITEMS || (required && values.is_empty()) {
        return Err(AppError::blocked(format!(
            "subagent result {label} count 범위 오류"
        )));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_bounded_text(value, label, 1, MAX_ITEM_BYTES)?;
        if !seen.insert(value) {
            return Err(AppError::blocked(format!(
                "subagent result {label} duplicate 차단"
            )));
        }
    }
    Ok(())
}

fn validate_bounded_text(
    value: &str,
    label: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), AppError> {
    if value.len() < minimum || value.len() > maximum || value.trim() != value {
        return Err(AppError::blocked(format!(
            "subagent result {label} byte/canonical 범위 오류"
        )));
    }
    Ok(())
}
