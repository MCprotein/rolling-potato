//! Canonical subagent evidence artifact codec and integrity policy.

use super::{EvidenceSourceBinding, SourcePointerBinding, SubagentRecordV1, SubagentResultV1};
use crate::foundation::error::AppError;
use crate::foundation::integrity;
use crate::foundation::serialization as strict_json;
use crate::foundation::serialization::{CanonicalObject, CanonicalValue};
use std::collections::BTreeSet;

const EVIDENCE_V2_KEYS: &[&str] = &[
    "schema_version",
    "evidence_id",
    "artifact_hash",
    "subagent_id",
    "parent_workflow_id",
    "result_artifact_id",
    "result_artifact_hash",
    "evidence_refs",
    "source_bindings",
];
const SOURCE_BINDING_KEYS: &[&str] = &["path", "stable_ref", "fingerprint"];

pub(crate) fn evidence_source_bindings(
    sources: &[SourcePointerBinding<'_>],
    evidence_refs: &[String],
) -> Result<Vec<EvidenceSourceBinding>, AppError> {
    evidence_refs
        .iter()
        .map(|stable_ref| {
            let pointer = sources
                .iter()
                .find(|pointer| pointer.stable_ref == stable_ref)
                .ok_or_else(|| {
                    AppError::blocked("subagent evidence source pointer binding 누락")
                })?;
            Ok(EvidenceSourceBinding {
                path: pointer.path.to_string(),
                stable_ref: pointer.stable_ref.to_string(),
                fingerprint: pointer.fingerprint.to_string(),
            })
        })
        .collect()
}

pub(crate) fn has_artifact_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 20 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

pub(crate) fn verify_evidence_artifact(
    record: &SubagentRecordV1,
    result: &SubagentResultV1,
    installed: &str,
) -> Result<Option<Vec<EvidenceSourceBinding>>, AppError> {
    let legacy_payload = render_evidence_payload_v1(
        &record.evidence_id,
        record,
        &record.result_artifact_id,
        &record.result_artifact_hash,
        &result.evidence_refs,
    );
    let legacy_hash = integrity::sha256_text(&legacy_payload);
    if legacy_hash == record.evidence_hash
        && installed == installable_evidence_body(&legacy_payload, &legacy_hash)
    {
        return Ok(None);
    }

    let object =
        strict_json::parse_canonical_object(installed, EVIDENCE_V2_KEYS, "subagent evidence v2")?;
    if strict_json::canonical_u64(&object, "schema_version", "subagent evidence v2")? != 2 {
        return Err(AppError::blocked(
            "subagent completed evidence schema version 불일치",
        ));
    }
    let sources = parse_source_bindings(&object)?;
    let evidence_refs = evidence_string_array(&object, "evidence_refs")?;
    if evidence_string(&object, "evidence_id")? != record.evidence_id
        || evidence_string(&object, "artifact_hash")? != record.evidence_hash
        || evidence_string(&object, "subagent_id")? != record.subagent_id
        || evidence_string(&object, "parent_workflow_id")? != record.parent_workflow_id
        || evidence_string(&object, "result_artifact_id")? != record.result_artifact_id
        || evidence_string(&object, "result_artifact_hash")? != record.result_artifact_hash
        || evidence_refs != result.evidence_refs
        || sources.len() != evidence_refs.len()
        || sources
            .iter()
            .zip(&evidence_refs)
            .any(|(source, reference)| source.stable_ref != *reference)
        || sources
            .iter()
            .any(|source| !record.read_paths.iter().any(|path| path == &source.path))
    {
        return Err(AppError::blocked(
            "subagent completed evidence v2 binding 불일치",
        ));
    }
    let payload = render_evidence_payload_v2(
        &record.evidence_id,
        record,
        &record.result_artifact_id,
        &record.result_artifact_hash,
        &evidence_refs,
        &sources,
    );
    let evidence_hash = integrity::sha256_text(&payload);
    if evidence_hash != record.evidence_hash
        || installed != installable_evidence_body(&payload, &evidence_hash)
    {
        return Err(AppError::blocked(
            "subagent completed evidence artifact binding 불일치",
        ));
    }
    Ok(Some(sources))
}

pub(crate) fn render_evidence_payload_v2(
    evidence_id: &str,
    record: &SubagentRecordV1,
    result_artifact_id: &str,
    result_artifact_hash: &str,
    evidence_refs: &[String],
    sources: &[EvidenceSourceBinding],
) -> String {
    format!(
        "{{\"schema_version\":2,\"evidence_id\":\"{}\",\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"result_artifact_id\":\"{}\",\"result_artifact_hash\":\"{}\",\"evidence_refs\":{},\"source_bindings\":{}}}",
        escape(evidence_id),
        escape(&record.subagent_id),
        escape(&record.parent_workflow_id),
        escape(result_artifact_id),
        result_artifact_hash,
        render_string_array(evidence_refs),
        render_source_bindings(sources),
    )
}

pub(crate) fn evidence_id(record: &SubagentRecordV1, result_artifact_hash: &str) -> String {
    format!(
        "evidence-subagent-{}",
        &integrity::sha256_text(&format!(
            "{}\n{}\n{}",
            record.subagent_id, record.parent_workflow_id, result_artifact_hash
        ))[..20]
    )
}

pub(crate) fn installable_evidence_body(evidence_payload: &str, evidence_hash: &str) -> String {
    evidence_payload.replacen(
        "\"subagent_id\"",
        &format!("\"artifact_hash\":\"{evidence_hash}\",\"subagent_id\""),
        1,
    )
}

fn render_evidence_payload_v1(
    evidence_id: &str,
    record: &SubagentRecordV1,
    result_artifact_id: &str,
    result_artifact_hash: &str,
    evidence_refs: &[String],
) -> String {
    format!(
        "{{\"schema_version\":1,\"evidence_id\":\"{}\",\"subagent_id\":\"{}\",\"parent_workflow_id\":\"{}\",\"result_artifact_id\":\"{}\",\"result_artifact_hash\":\"{}\",\"evidence_refs\":{}}}",
        escape(evidence_id),
        escape(&record.subagent_id),
        escape(&record.parent_workflow_id),
        escape(result_artifact_id),
        result_artifact_hash,
        render_string_array(evidence_refs),
    )
}

fn parse_source_bindings(object: &CanonicalObject) -> Result<Vec<EvidenceSourceBinding>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get("source_bindings") else {
        return Err(AppError::blocked(
            "subagent evidence source_bindings type 오류",
        ));
    };
    let mut seen = BTreeSet::new();
    values
        .iter()
        .map(|value| {
            let CanonicalValue::Object(source) = value else {
                return Err(AppError::blocked(
                    "subagent evidence source binding item type 오류",
                ));
            };
            let actual = source
                .entries
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>();
            if actual != SOURCE_BINDING_KEYS {
                return Err(AppError::blocked(
                    "subagent evidence source binding key order 불일치",
                ));
            }
            let binding = EvidenceSourceBinding {
                path: evidence_string(source, "path")?,
                stable_ref: evidence_string(source, "stable_ref")?,
                fingerprint: evidence_string(source, "fingerprint")?,
            };
            if binding.path.is_empty()
                || binding.stable_ref.is_empty()
                || !is_sha256(&binding.fingerprint)
                || !seen.insert(binding.stable_ref.clone())
            {
                return Err(AppError::blocked(
                    "subagent evidence source binding canonical 값 오류",
                ));
            }
            Ok(binding)
        })
        .collect()
}

fn evidence_string(object: &CanonicalObject, key: &str) -> Result<String, AppError> {
    match object.get(key) {
        Some(CanonicalValue::String(value)) => Ok(value.clone()),
        _ => Err(AppError::blocked(format!(
            "subagent evidence missing/wrong string: {key}"
        ))),
    }
}

fn evidence_string_array(object: &CanonicalObject, key: &str) -> Result<Vec<String>, AppError> {
    let Some(CanonicalValue::Array(values)) = object.get(key) else {
        return Err(AppError::blocked(format!(
            "subagent evidence missing/wrong array: {key}"
        )));
    };
    values
        .iter()
        .map(|value| match value {
            CanonicalValue::String(value) => Ok(value.clone()),
            _ => Err(AppError::blocked(format!(
                "subagent evidence array item type 오류: {key}"
            ))),
        })
        .collect()
}

fn render_source_bindings(sources: &[EvidenceSourceBinding]) -> String {
    format!(
        "[{}]",
        sources
            .iter()
            .map(|source| format!(
                "{{\"path\":\"{}\",\"stable_ref\":\"{}\",\"fingerprint\":\"{}\"}}",
                escape(&source.path),
                escape(&source.stable_ref),
                source.fingerprint,
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn render_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", escape(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn escape(value: &str) -> String {
    strict_json::escape_string_content(value)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
