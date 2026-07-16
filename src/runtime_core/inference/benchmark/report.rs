use crate::foundation::serialization as strict_json;
use crate::runtime_core::inference::backend::BackendChatRun;
use crate::runtime_core::inference::model::manifest::quantization_for_artifact_hash;

use super::fixture::{BenchmarkFixture, BenchmarkPromptArtifact};
use super::BenchmarkScore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BenchmarkReportFormat {
    Jsonl,
}

pub(crate) fn reproducibility_manifest_json(
    fixture: &BenchmarkFixture,
    benchmark_run_id: &str,
    recorded_at_ms: u128,
) -> String {
    format!(
        "{{\"harness_ref\":\"{}\",\"benchmark_run_id\":\"{}\",\"fixture_id\":\"{}\",\"fixture_sha256\":\"{}\",\"runner_command\":\"{}\",\"run_count\":0,\"retry_count\":0,\"seed_policy\":\"{}\",\"sampling_options\":\"{}\",\"os\":\"{}\",\"arch\":\"{}\",\"hardware_note\":\"{}\",\"ram_note\":\"{}\",\"power_thermal_note\":\"{}\",\"backend_id\":\"{}\",\"backend_version\":\"{}\",\"model_id\":\"{}\",\"model_artifact_hash\":\"{}\",\"quantization\":\"{}\",\"prompt_runtime_version\":\"{}\",\"tool_policy_version\":\"{}\",\"ontology_view\":\"{}\",\"context_budget\":{},\"expected_escalation_target\":\"{}\",\"abstention_required\":{},\"redaction_status\":\"redacted\",\"raw_artifact_retention_policy\":\"{}\",\"recorded_at_ms\":{}}}",
        strict_json::escape_string_content(&harness_ref()),
        strict_json::escape_string_content(benchmark_run_id),
        strict_json::escape_string_content(&fixture.fixture_id),
        strict_json::escape_string_content(&fixture.sha256),
        strict_json::escape_string_content("rpotato benchmark record --fixture <path>"),
        strict_json::escape_string_content(&fixture.seed_policy),
        strict_json::escape_string_content(&fixture.sampling_options),
        strict_json::escape_string_content(std::env::consts::OS),
        strict_json::escape_string_content(std::env::consts::ARCH),
        strict_json::escape_string_content("not-recorded"),
        strict_json::escape_string_content("not-recorded"),
        strict_json::escape_string_content("not-recorded"),
        strict_json::escape_string_content(&fixture.backend_id),
        strict_json::escape_string_content(&fixture.backend_version),
        strict_json::escape_string_content(&fixture.model_id),
        strict_json::escape_string_content(&fixture.model_artifact_hash),
        strict_json::escape_string_content(&fixture.quantization),
        strict_json::escape_string_content(&fixture.prompt_runtime_version),
        strict_json::escape_string_content(&fixture.tool_policy_version),
        strict_json::escape_string_content(&fixture.ontology_view),
        fixture.context_budget,
        strict_json::escape_string_content(&fixture.expected_escalation_target),
        fixture.abstention_required,
        strict_json::escape_string_content(&fixture.raw_artifact_retention_policy),
        recorded_at_ms
    )
}

pub(crate) fn redacted_report_json(fixture: &BenchmarkFixture, benchmark_run_id: &str) -> String {
    format!(
        "{{\"benchmark_run_id\":\"{}\",\"fixture_id\":\"{}\",\"benchmark_name\":\"{}\",\"runtime_capability_under_test\":\"{}\",\"expected_policy_decision\":\"{}\",\"expected_escalation_target\":\"{}\",\"required_tools\":{},\"required_source_reads\":{},\"required_evidence_records\":{},\"abstention_required\":{},\"expected_failure_category\":\"{}\",\"claim_state\":\"not-comparable\",\"score\":null,\"raw_prompt_source_stored\":false}}",
        strict_json::escape_string_content(benchmark_run_id),
        strict_json::escape_string_content(&fixture.fixture_id),
        strict_json::escape_string_content(&fixture.benchmark_name),
        strict_json::escape_string_content(&fixture.runtime_capability_under_test),
        strict_json::escape_string_content(&fixture.expected_policy_decision),
        strict_json::escape_string_content(&fixture.expected_escalation_target),
        json_string_array(&fixture.required_tools),
        json_string_array(&fixture.required_source_reads),
        json_string_array(&fixture.required_evidence_records),
        fixture.abstention_required,
        strict_json::escape_string_content(&fixture.expected_failure_category)
    )
}

pub(crate) fn executable_reproducibility_manifest_json(
    fixture: &BenchmarkFixture,
    prompt: &BenchmarkPromptArtifact,
    run: &BackendChatRun,
    score: &BenchmarkScore,
    benchmark_run_id: &str,
    model_run_id: &str,
    recorded_at_ms: u128,
) -> String {
    let sampling_options = format!(
        "temperature={},top_p={},requested_max_tokens={},effective_max_tokens={}",
        run.sampling.temperature,
        run.sampling.top_p,
        run.requested_max_tokens,
        run.effective_max_tokens
    );
    let quantization =
        quantization_for_artifact_hash(&run.model_artifact_hash).unwrap_or("unresolved");
    format!(
        "{{\"harness_ref\":\"{}\",\"benchmark_run_id\":\"{}\",\"model_run_id\":\"{}\",\"fixture_id\":\"{}\",\"fixture_sha256\":\"{}\",\"prompt_artifact_sha256\":\"{}\",\"prompt_chars\":{},\"runner_command\":\"{}\",\"run_count\":1,\"retry_count\":0,\"seed_policy\":\"{}\",\"sampling_options\":\"{}\",\"os\":\"{}\",\"arch\":\"{}\",\"hardware_note\":\"{}\",\"ram_note\":\"{}\",\"power_thermal_note\":\"{}\",\"backend_id\":\"{}\",\"backend_version\":\"{}\",\"model_id\":\"{}\",\"model_artifact_hash\":\"{}\",\"quantization\":\"{}\",\"prompt_runtime_version\":\"{}\",\"tool_policy_version\":\"{}\",\"ontology_view\":\"{}\",\"context_budget\":{},\"expected_escalation_target\":\"{}\",\"abstention_required\":{},\"score\":{},\"score_unit\":\"0-3-local-product-score\",\"minimum_score\":{},\"local_pass\":{},\"expected_matches\":{},\"expected_total\":{},\"forbidden_matches\":{},\"latency_ms\":{},\"tokens_per_second\":{},\"prompt_tokens\":{},\"completion_tokens\":{},\"total_tokens\":{},\"resource_pressure\":\"{}\",\"peak_rss_bytes\":{},\"redaction_status\":\"redacted\",\"raw_artifact_retention_policy\":\"{}\",\"raw_prompt_source_stored\":false,\"public_benchmark_parity\":\"not-claimed\",\"recorded_at_ms\":{}}}",
        strict_json::escape_string_content(&harness_ref()),
        strict_json::escape_string_content(benchmark_run_id),
        strict_json::escape_string_content(model_run_id),
        strict_json::escape_string_content(&fixture.fixture_id),
        strict_json::escape_string_content(&fixture.sha256),
        strict_json::escape_string_content(&prompt.sha256),
        prompt.chars,
        strict_json::escape_string_content("rpotato benchmark run --fixture <path> --prompt <artifact>"),
        strict_json::escape_string_content(&fixture.seed_policy),
        strict_json::escape_string_content(&sampling_options),
        strict_json::escape_string_content(std::env::consts::OS),
        strict_json::escape_string_content(std::env::consts::ARCH),
        strict_json::escape_string_content("not-recorded"),
        strict_json::escape_string_content("not-recorded"),
        strict_json::escape_string_content("not-recorded"),
        strict_json::escape_string_content(&run.backend_id),
        strict_json::escape_string_content(&run.backend_version),
        strict_json::escape_string_content(&run.model_id),
        strict_json::escape_string_content(&run.model_artifact_hash),
        strict_json::escape_string_content(quantization),
        strict_json::escape_string_content(&fixture.prompt_runtime_version),
        strict_json::escape_string_content(&fixture.tool_policy_version),
        strict_json::escape_string_content(&fixture.ontology_view),
        fixture.context_budget,
        strict_json::escape_string_content(&fixture.expected_escalation_target),
        fixture.abstention_required,
        score.score,
        fixture.minimum_score.unwrap_or(2),
        score.local_pass,
        score.expected_matches,
        score.expected_total,
        score.forbidden_matches,
        run.elapsed_ms,
        json_option_f64(local_tokens_per_second(run)),
        json_option_u32(run.prompt_tokens),
        json_option_u32(run.completion_tokens),
        json_option_u32(run.total_tokens),
        strict_json::escape_string_content(&run.resource_pressure),
        json_option_u64(run.resource_peak_rss_bytes),
        strict_json::escape_string_content(&fixture.raw_artifact_retention_policy),
        recorded_at_ms
    )
}

pub(crate) fn executable_redacted_report_json(
    fixture: &BenchmarkFixture,
    prompt: &BenchmarkPromptArtifact,
    run: &BackendChatRun,
    score: &BenchmarkScore,
    benchmark_run_id: &str,
    model_run_id: &str,
) -> String {
    format!(
        "{{\"benchmark_run_id\":\"{}\",\"model_run_id\":\"{}\",\"fixture_id\":\"{}\",\"benchmark_name\":\"{}\",\"runtime_capability_under_test\":\"{}\",\"ontology_view\":\"{}\",\"prompt_artifact_sha256\":\"{}\",\"prompt_chars\":{},\"response_chars\":{},\"expected_policy_decision\":\"{}\",\"expected_escalation_target\":\"{}\",\"required_tools\":{},\"required_source_reads\":{},\"required_evidence_records\":{},\"abstention_required\":{},\"expected_failure_category\":\"{}\",\"claim_state\":\"measured-locally\",\"score\":{},\"score_unit\":\"0-3-local-product-score\",\"minimum_score\":{},\"local_pass\":{},\"expected_matches\":{},\"expected_total\":{},\"forbidden_matches\":{},\"abstention_ok\":{},\"matched_expected\":{},\"matched_forbidden\":{},\"latency_ms\":{},\"tokens_per_second\":{},\"prompt_tokens\":{},\"completion_tokens\":{},\"total_tokens\":{},\"resource_pressure\":\"{}\",\"peak_rss_bytes\":{},\"raw_prompt_source_stored\":false,\"public_benchmark_parity\":\"not-claimed\"}}",
        strict_json::escape_string_content(benchmark_run_id),
        strict_json::escape_string_content(model_run_id),
        strict_json::escape_string_content(&fixture.fixture_id),
        strict_json::escape_string_content(&fixture.benchmark_name),
        strict_json::escape_string_content(&fixture.runtime_capability_under_test),
        strict_json::escape_string_content(&fixture.ontology_view),
        strict_json::escape_string_content(&prompt.sha256),
        prompt.chars,
        run.response_chars,
        strict_json::escape_string_content(&fixture.expected_policy_decision),
        strict_json::escape_string_content(&fixture.expected_escalation_target),
        json_string_array(&fixture.required_tools),
        json_string_array(&fixture.required_source_reads),
        json_string_array(&fixture.required_evidence_records),
        fixture.abstention_required,
        strict_json::escape_string_content(&fixture.expected_failure_category),
        score.score,
        fixture.minimum_score.unwrap_or(2),
        score.local_pass,
        score.expected_matches,
        score.expected_total,
        score.forbidden_matches,
        score.abstention_ok,
        json_string_array(&score.matched_expected),
        json_string_array(&score.matched_forbidden),
        run.elapsed_ms,
        json_option_f64(local_tokens_per_second(run)),
        json_option_u32(run.prompt_tokens),
        json_option_u32(run.completion_tokens),
        json_option_u32(run.total_tokens),
        strict_json::escape_string_content(&run.resource_pressure),
        json_option_u64(run.resource_peak_rss_bytes)
    )
}

pub(crate) fn json_option(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|value| format!("\"{}\"", strict_json::escape_string_content(value)))
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn json_option_bool(value: Option<bool>) -> String {
    value
        .map(|value| {
            if value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        })
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn json_option_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn json_option_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn json_option_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.6}"))
        .unwrap_or_else(|| "null".to_string())
}

pub(crate) fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

pub(crate) fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

pub(crate) fn local_tokens_per_second(run: &BackendChatRun) -> Option<f64> {
    let completion_tokens = run.completion_tokens?;
    if completion_tokens == 0 || run.elapsed_ms == 0 {
        return None;
    }
    Some((completion_tokens as f64) / ((run.elapsed_ms as f64) / 1000.0))
}

pub(crate) fn json_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", strict_json::escape_string_content(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub(crate) fn json_raw_or_string(value: &str) -> String {
    let trimmed = value.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        trimmed.to_string()
    } else {
        format!("\"{}\"", strict_json::escape_string_content(value))
    }
}

pub(crate) fn harness_ref() -> String {
    format!("rpotato-benchmark-harness@{}", env!("CARGO_PKG_VERSION"))
}
