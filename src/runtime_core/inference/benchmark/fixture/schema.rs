use std::collections::BTreeMap;

use crate::foundation::error::AppError;

use super::types::{BenchmarkFixture, FixtureJsonValue};

pub(super) fn validate_fixture_semantics(fixture: &BenchmarkFixture) -> Result<(), AppError> {
    if !matches!(
        fixture.expected_policy_decision.as_str(),
        "allow" | "ask" | "deny"
    ) {
        return Err(AppError::usage(
            "expected_policy_decision은 allow, ask, deny 중 하나여야 합니다.",
        ));
    }

    if !matches!(
        fixture.expected_failure_category.as_str(),
        "none"
            | "model-output-failure"
            | "prompt-context-packing-failure"
            | "ontology-source-pointer-failure"
            | "runtime-policy-parser-failure"
            | "tool-command-failure"
            | "backend-runtime-failure"
            | "fixture-issue"
    ) {
        return Err(AppError::usage(
            "expected_failure_category 값이 benchmark failure taxonomy에 없습니다.",
        ));
    }

    if !matches!(
        fixture.raw_artifact_retention_policy.as_str(),
        "none" | "redacted-only"
    ) {
        return Err(AppError::usage(
            "raw_artifact_retention_policy는 none 또는 redacted-only여야 합니다.",
        ));
    }

    if fixture.context_budget == 0 {
        return Err(AppError::usage("context_budget은 1 이상이어야 합니다."));
    }

    if fixture.minimum_score.is_some_and(|score| score > 3) {
        return Err(AppError::usage("minimum_score는 0부터 3 사이여야 합니다."));
    }

    Ok(())
}

pub(super) fn required_string(
    fields: &BTreeMap<String, FixtureJsonValue>,
    key: &str,
) -> Result<String, AppError> {
    let Some(FixtureJsonValue::String(value)) = fields.get(key) else {
        return Err(AppError::usage(format!(
            "benchmark fixture에 필수 string field가 없거나 type이 다릅니다: {key}"
        )));
    };
    if value.trim().is_empty() {
        return Err(AppError::usage(format!(
            "benchmark fixture field는 비어 있을 수 없습니다: {key}"
        )));
    }
    Ok(value.clone())
}

pub(super) fn required_u32(
    fields: &BTreeMap<String, FixtureJsonValue>,
    key: &str,
) -> Result<u32, AppError> {
    let Some(FixtureJsonValue::U32(value)) = fields.get(key) else {
        return Err(AppError::usage(format!(
            "benchmark fixture에 필수 positive integer field가 없거나 type이 다릅니다: {key}"
        )));
    };
    Ok(*value)
}

pub(super) fn required_bool(
    fields: &BTreeMap<String, FixtureJsonValue>,
    key: &str,
) -> Result<bool, AppError> {
    let Some(FixtureJsonValue::Bool(value)) = fields.get(key) else {
        return Err(AppError::usage(format!(
            "benchmark fixture에 필수 bool field가 없거나 type이 다릅니다: {key}"
        )));
    };
    Ok(*value)
}

pub(super) fn required_string_array(
    fields: &BTreeMap<String, FixtureJsonValue>,
    key: &str,
) -> Result<Vec<String>, AppError> {
    let Some(FixtureJsonValue::StringArray(values)) = fields.get(key) else {
        return Err(AppError::usage(format!(
            "benchmark fixture에 필수 string array field가 없거나 type이 다릅니다: {key}"
        )));
    };
    validate_non_empty_items(values, key)?;
    Ok(values.clone())
}

pub(super) fn optional_string_array(
    fields: &BTreeMap<String, FixtureJsonValue>,
    key: &str,
) -> Result<Vec<String>, AppError> {
    let Some(value) = fields.get(key) else {
        return Ok(Vec::new());
    };
    let FixtureJsonValue::StringArray(values) = value else {
        return Err(AppError::usage(format!(
            "benchmark fixture optional field의 type이 다릅니다: {key}"
        )));
    };
    validate_non_empty_items(values, key)?;
    Ok(values.clone())
}

fn validate_non_empty_items(values: &[String], key: &str) -> Result<(), AppError> {
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(AppError::usage(format!(
            "benchmark fixture array field에는 빈 문자열을 넣을 수 없습니다: {key}"
        )));
    }
    Ok(())
}

pub(super) fn optional_u32(
    fields: &BTreeMap<String, FixtureJsonValue>,
    key: &str,
) -> Result<Option<u32>, AppError> {
    let Some(value) = fields.get(key) else {
        return Ok(None);
    };
    let FixtureJsonValue::U32(value) = value else {
        return Err(AppError::usage(format!(
            "benchmark fixture optional field의 type이 다릅니다: {key}"
        )));
    };
    Ok(Some(*value))
}

pub(super) fn validate_fixture_schema(
    fields: &BTreeMap<String, FixtureJsonValue>,
) -> Result<(), AppError> {
    let expected = expected_fixture_fields();
    for key in fields.keys() {
        if forbidden_fixture_field(key) {
            return Err(AppError::usage(format!(
                "benchmark fixture에는 raw prompt/source field를 넣을 수 없습니다: {key}"
            )));
        }
        if !expected.contains(&key.as_str()) {
            return Err(AppError::usage(format!(
                "benchmark fixture에 지원하지 않는 field가 있습니다: {key}"
            )));
        }
    }
    Ok(())
}

fn expected_fixture_fields() -> &'static [&'static str] {
    &[
        "fixture_id",
        "benchmark_name",
        "runtime_capability_under_test",
        "model_vs_runtime_responsibility",
        "expected_route",
        "expected_policy_decision",
        "expected_escalation_target",
        "required_tools",
        "required_source_reads",
        "required_evidence_records",
        "abstention_required",
        "expected_failure_category",
        "ontology_view",
        "context_budget",
        "model_id",
        "model_artifact_hash",
        "quantization",
        "backend_id",
        "backend_version",
        "dataset_ref",
        "prompt_runtime_version",
        "tool_policy_version",
        "seed_policy",
        "sampling_options",
        "raw_artifact_retention_policy",
        "expected_response_contains",
        "forbidden_response_contains",
        "minimum_score",
    ]
}

fn forbidden_fixture_field(key: &str) -> bool {
    matches!(
        key,
        "prompt"
            | "raw_prompt"
            | "source"
            | "source_text"
            | "source_code"
            | "raw_source"
            | "response"
            | "raw_response"
            | "transcript"
            | "raw_transcript"
            | "command_output"
            | "raw_command_output"
            | "log_text"
            | "raw_log"
    )
}
