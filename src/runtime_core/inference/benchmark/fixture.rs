//! Benchmark fixture domain facade.

use std::path::PathBuf;

use crate::foundation::error::AppError;

mod json;
mod schema;
mod types;

use json::parse_fixture_json_object;
use schema::{
    optional_string_array, optional_u32, required_bool, required_string, required_string_array,
    required_u32, validate_fixture_schema, validate_fixture_semantics,
};
pub(crate) use types::{BenchmarkFixture, BenchmarkPromptArtifact};

pub(crate) fn parse_fixture(
    text: &str,
    path: PathBuf,
    sha256: String,
) -> Result<BenchmarkFixture, AppError> {
    if !text.trim_start().starts_with('{') || !text.trim_end().ends_with('}') {
        return Err(AppError::usage(
            "benchmark fixture는 JSON object metadata여야 합니다.",
        ));
    }

    let fields = parse_fixture_json_object(text)?;
    validate_fixture_schema(&fields)?;

    let fixture = BenchmarkFixture {
        path,
        sha256,
        fixture_id: required_string(&fields, "fixture_id")?,
        benchmark_name: required_string(&fields, "benchmark_name")?,
        runtime_capability_under_test: required_string(&fields, "runtime_capability_under_test")?,
        model_vs_runtime_responsibility: required_string(
            &fields,
            "model_vs_runtime_responsibility",
        )?,
        expected_route: required_string(&fields, "expected_route")?,
        expected_policy_decision: required_string(&fields, "expected_policy_decision")?,
        expected_escalation_target: required_string(&fields, "expected_escalation_target")?,
        required_tools: required_string_array(&fields, "required_tools")?,
        required_source_reads: required_string_array(&fields, "required_source_reads")?,
        required_evidence_records: required_string_array(&fields, "required_evidence_records")?,
        abstention_required: required_bool(&fields, "abstention_required")?,
        expected_failure_category: required_string(&fields, "expected_failure_category")?,
        ontology_view: required_string(&fields, "ontology_view")?,
        context_budget: required_u32(&fields, "context_budget")?,
        model_id: required_string(&fields, "model_id")?,
        model_artifact_hash: required_string(&fields, "model_artifact_hash")?,
        quantization: required_string(&fields, "quantization")?,
        backend_id: required_string(&fields, "backend_id")?,
        backend_version: required_string(&fields, "backend_version")?,
        dataset_ref: required_string(&fields, "dataset_ref")?,
        prompt_runtime_version: required_string(&fields, "prompt_runtime_version")?,
        tool_policy_version: required_string(&fields, "tool_policy_version")?,
        seed_policy: required_string(&fields, "seed_policy")?,
        sampling_options: required_string(&fields, "sampling_options")?,
        raw_artifact_retention_policy: required_string(&fields, "raw_artifact_retention_policy")?,
        expected_response_contains: optional_string_array(&fields, "expected_response_contains")?,
        forbidden_response_contains: optional_string_array(&fields, "forbidden_response_contains")?,
        minimum_score: optional_u32(&fields, "minimum_score")?,
    };

    validate_fixture_semantics(&fixture)?;
    Ok(fixture)
}
