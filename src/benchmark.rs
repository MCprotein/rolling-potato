use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::app::AppError;
use crate::{checksum, ledger, observability, paths};

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkFixture {
    path: PathBuf,
    sha256: String,
    fixture_id: String,
    benchmark_name: String,
    runtime_capability_under_test: String,
    model_vs_runtime_responsibility: String,
    expected_route: String,
    expected_policy_decision: String,
    expected_escalation_target: String,
    required_tools: Vec<String>,
    required_source_reads: Vec<String>,
    required_evidence_records: Vec<String>,
    abstention_required: bool,
    expected_failure_category: String,
    ontology_view: String,
    context_budget: u32,
    model_id: String,
    model_artifact_hash: String,
    quantization: String,
    backend_id: String,
    backend_version: String,
    dataset_ref: String,
    prompt_runtime_version: String,
    tool_policy_version: String,
    seed_policy: String,
    sampling_options: String,
    raw_artifact_retention_policy: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkReportFormat {
    Jsonl,
}

pub fn validate_report(path: &str) -> Result<String, AppError> {
    let fixture = read_fixture(path)?;
    Ok(format!(
        "benchmark fixture validation\n- status: valid\n- fixture id: {}\n- benchmark: {}\n- fixture path: {}\n- fixture sha256: {}\n- runtime capability: {}\n- responsibility split: {}\n- expected route: {}\n- expected policy decision: {}\n- expected escalation target: {}\n- required tools: {}\n- required source reads: {}\n- required evidence records: {}\n- abstention required: {}\n- expected failure category: {}\n- ontology view: {}\n- context budget tokens: {}\n- model id: {}\n- backend: {} {}\n- dataset: {}\n- reproducibility metadata: ready\n- raw prompt/source 저장: 없음\n- boundary: fixture metadata validation only; model 실행, scoring, public benchmark parity claim을 수행하지 않음",
        fixture.fixture_id,
        fixture.benchmark_name,
        fixture.path.display(),
        fixture.sha256,
        fixture.runtime_capability_under_test,
        fixture.model_vs_runtime_responsibility,
        fixture.expected_route,
        fixture.expected_policy_decision,
        fixture.expected_escalation_target,
        fixture.required_tools.len(),
        fixture.required_source_reads.len(),
        fixture.required_evidence_records.len(),
        if fixture.abstention_required { "yes" } else { "no" },
        fixture.expected_failure_category,
        fixture.ontology_view,
        fixture.context_budget,
        fixture.model_id,
        fixture.backend_id,
        fixture.backend_version,
        fixture.dataset_ref
    ))
}

pub fn record_report(path: &str) -> Result<String, AppError> {
    let fixture = read_fixture(path)?;
    let identity = ledger::current_identity();
    let event = ledger::new_event_for(
        &identity,
        "benchmark.run.recorded",
        "benchmark fixture metadata recorded",
        &format!(
            "fixture_id={} benchmark={} fixture_sha256={} model_id={} backend_id={} harness_ref={} claim_state=not-comparable",
            fixture.fixture_id,
            fixture.benchmark_name,
            fixture.sha256,
            fixture.model_id,
            fixture.backend_id,
            harness_ref()
        ),
    );
    let benchmark_run_id = format!("benchmark-{}", event.event_id);
    let manifest = reproducibility_manifest_json(&fixture, &benchmark_run_id, event.ts_ms);
    let redacted_report = redacted_report_json(&fixture, &benchmark_run_id);
    let metric = observability::BenchmarkRunMetric {
        benchmark_run_id: benchmark_run_id.clone(),
        session_id: identity.session_id.clone(),
        model_id: fixture.model_id.clone(),
        benchmark_name: fixture.benchmark_name.clone(),
        fixture_id: fixture.fixture_id.clone(),
        fixture_sha256: fixture.sha256.clone(),
        claim_state: "not-comparable".to_string(),
        score: None,
        score_unit: None,
        harness_ref: harness_ref(),
        dataset_ref: Some(fixture.dataset_ref.clone()),
        backend_id: Some(fixture.backend_id.clone()),
        reproducibility_manifest: manifest,
        redacted_report,
        recorded_at_ms: event.ts_ms,
    };

    ledger::append_event(&event)?;
    observability::project_event(&event)?;
    observability::record_benchmark_run(&metric)?;

    Ok(format!(
        "benchmark run 기록\n- status: recorded\n- benchmark run id: {}\n- session id: {}\n- fixture id: {}\n- benchmark: {}\n- fixture sha256: {}\n- harness ref: {}\n- claim state: not-comparable\n- score: 없음\n- ledger event: {}\n- SQLite projection: benchmark_runs\n- raw prompt/source 저장: 없음\n- boundary: fixture metadata와 reproducibility manifest만 기록했습니다. model 실행, scoring, public benchmark parity claim은 수행하지 않았습니다.",
        benchmark_run_id,
        identity.session_id,
        fixture.fixture_id,
        fixture.benchmark_name,
        fixture.sha256,
        harness_ref(),
        event.event_id
    ))
}

pub fn report_export(format: BenchmarkReportFormat) -> Result<String, AppError> {
    match format {
        BenchmarkReportFormat::Jsonl => {
            let rows = observability::benchmark_run_reports()?;
            let mut output = String::new();
            for row in rows {
                output.push_str(&format!(
                    "{{\"benchmark_run_id\":\"{}\",\"session_id\":\"{}\",\"model_id\":\"{}\",\"benchmark_name\":\"{}\",\"fixture_id\":\"{}\",\"fixture_sha256\":\"{}\",\"claim_state\":\"{}\",\"score\":{},\"score_unit\":{},\"harness_ref\":\"{}\",\"dataset_ref\":{},\"backend_id\":{},\"recorded_at_ms\":{},\"reproducibility_manifest\":{},\"redacted_report\":{}}}\n",
                    ledger::json_string(&row.benchmark_run_id),
                    ledger::json_string(&row.session_id),
                    ledger::json_string(&row.model_id),
                    ledger::json_string(&row.benchmark_name),
                    ledger::json_string(&row.fixture_id),
                    ledger::json_string(&row.fixture_sha256),
                    ledger::json_string(&row.claim_state),
                    row.score
                        .map(|score| format!("{score:.6}"))
                        .unwrap_or_else(|| "null".to_string()),
                    json_option(&row.score_unit),
                    ledger::json_string(&row.harness_ref),
                    json_option(&row.dataset_ref),
                    json_option(&row.backend_id),
                    row.recorded_at_ms,
                    json_raw_or_string(&row.reproducibility_manifest),
                    json_raw_or_string(&row.redacted_report)
                ));
            }
            Ok(output)
        }
    }
}

fn read_fixture(path: &str) -> Result<BenchmarkFixture, AppError> {
    let path = project_local_file(path)?;
    let text = fs::read_to_string(&path).map_err(|err| {
        AppError::runtime(format!(
            "benchmark fixture를 읽지 못했습니다: {} ({err})",
            path.display()
        ))
    })?;

    if !text.trim_start().starts_with('{') || !text.trim_end().ends_with('}') {
        return Err(AppError::usage(
            "benchmark fixture는 JSON object metadata여야 합니다.",
        ));
    }

    let fields = parse_fixture_json_object(&text)?;
    validate_fixture_schema(&fields)?;

    let fixture = BenchmarkFixture {
        sha256: checksum::sha256_file(&path)?,
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
        path,
    };

    validate_fixture_semantics(&fixture)?;
    Ok(fixture)
}

fn validate_fixture_semantics(fixture: &BenchmarkFixture) -> Result<(), AppError> {
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

    Ok(())
}

fn project_local_file(path: &str) -> Result<PathBuf, AppError> {
    if path.starts_with("http://") || path.starts_with("https://") {
        return Err(AppError::usage(
            "benchmark fixture path는 remote URL일 수 없습니다.",
        ));
    }

    let project_root = paths::project_root().canonicalize().map_err(|err| {
        AppError::runtime(format!(
            "project root를 확인하지 못했습니다: {} ({err})",
            paths::project_root().display()
        ))
    })?;
    let candidate = Path::new(path);
    let full_path = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    };
    let canonical = full_path.canonicalize().map_err(|err| {
        AppError::usage(format!(
            "benchmark fixture path를 찾지 못했습니다: {} ({err})",
            full_path.display()
        ))
    })?;
    if !canonical.starts_with(&project_root) {
        return Err(AppError::usage(
            "benchmark fixture는 project root 안의 파일이어야 합니다.",
        ));
    }
    let metadata = fs::metadata(&canonical).map_err(|err| {
        AppError::runtime(format!(
            "benchmark fixture metadata를 읽지 못했습니다: {} ({err})",
            canonical.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(AppError::usage(
            "benchmark fixture path는 파일이어야 합니다.",
        ));
    }
    Ok(canonical)
}

fn reproducibility_manifest_json(
    fixture: &BenchmarkFixture,
    benchmark_run_id: &str,
    recorded_at_ms: u128,
) -> String {
    format!(
        "{{\"harness_ref\":\"{}\",\"benchmark_run_id\":\"{}\",\"fixture_id\":\"{}\",\"fixture_sha256\":\"{}\",\"runner_command\":\"{}\",\"run_count\":0,\"retry_count\":0,\"seed_policy\":\"{}\",\"sampling_options\":\"{}\",\"os\":\"{}\",\"arch\":\"{}\",\"hardware_note\":\"{}\",\"ram_note\":\"{}\",\"power_thermal_note\":\"{}\",\"backend_id\":\"{}\",\"backend_version\":\"{}\",\"model_id\":\"{}\",\"model_artifact_hash\":\"{}\",\"quantization\":\"{}\",\"prompt_runtime_version\":\"{}\",\"tool_policy_version\":\"{}\",\"ontology_view\":\"{}\",\"context_budget\":{},\"expected_escalation_target\":\"{}\",\"abstention_required\":{},\"redaction_status\":\"redacted\",\"raw_artifact_retention_policy\":\"{}\",\"recorded_at_ms\":{}}}",
        ledger::json_string(&harness_ref()),
        ledger::json_string(benchmark_run_id),
        ledger::json_string(&fixture.fixture_id),
        ledger::json_string(&fixture.sha256),
        ledger::json_string("rpotato benchmark record --fixture <path>"),
        ledger::json_string(&fixture.seed_policy),
        ledger::json_string(&fixture.sampling_options),
        ledger::json_string(std::env::consts::OS),
        ledger::json_string(std::env::consts::ARCH),
        ledger::json_string("not-recorded"),
        ledger::json_string("not-recorded"),
        ledger::json_string("not-recorded"),
        ledger::json_string(&fixture.backend_id),
        ledger::json_string(&fixture.backend_version),
        ledger::json_string(&fixture.model_id),
        ledger::json_string(&fixture.model_artifact_hash),
        ledger::json_string(&fixture.quantization),
        ledger::json_string(&fixture.prompt_runtime_version),
        ledger::json_string(&fixture.tool_policy_version),
        ledger::json_string(&fixture.ontology_view),
        fixture.context_budget,
        ledger::json_string(&fixture.expected_escalation_target),
        fixture.abstention_required,
        ledger::json_string(&fixture.raw_artifact_retention_policy),
        recorded_at_ms
    )
}

fn redacted_report_json(fixture: &BenchmarkFixture, benchmark_run_id: &str) -> String {
    format!(
        "{{\"benchmark_run_id\":\"{}\",\"fixture_id\":\"{}\",\"benchmark_name\":\"{}\",\"runtime_capability_under_test\":\"{}\",\"expected_policy_decision\":\"{}\",\"expected_escalation_target\":\"{}\",\"required_tools\":{},\"required_source_reads\":{},\"required_evidence_records\":{},\"abstention_required\":{},\"expected_failure_category\":\"{}\",\"claim_state\":\"not-comparable\",\"score\":null,\"raw_prompt_source_stored\":false}}",
        ledger::json_string(benchmark_run_id),
        ledger::json_string(&fixture.fixture_id),
        ledger::json_string(&fixture.benchmark_name),
        ledger::json_string(&fixture.runtime_capability_under_test),
        ledger::json_string(&fixture.expected_policy_decision),
        ledger::json_string(&fixture.expected_escalation_target),
        json_string_array(&fixture.required_tools),
        json_string_array(&fixture.required_source_reads),
        json_string_array(&fixture.required_evidence_records),
        fixture.abstention_required,
        ledger::json_string(&fixture.expected_failure_category)
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FixtureJsonValue {
    String(String),
    U32(u32),
    Bool(bool),
    StringArray(Vec<String>),
}

fn required_string(
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

fn required_u32(fields: &BTreeMap<String, FixtureJsonValue>, key: &str) -> Result<u32, AppError> {
    let Some(FixtureJsonValue::U32(value)) = fields.get(key) else {
        return Err(AppError::usage(format!(
            "benchmark fixture에 필수 positive integer field가 없거나 type이 다릅니다: {key}"
        )));
    };
    Ok(*value)
}

fn required_bool(fields: &BTreeMap<String, FixtureJsonValue>, key: &str) -> Result<bool, AppError> {
    let Some(FixtureJsonValue::Bool(value)) = fields.get(key) else {
        return Err(AppError::usage(format!(
            "benchmark fixture에 필수 bool field가 없거나 type이 다릅니다: {key}"
        )));
    };
    Ok(*value)
}

fn required_string_array(
    fields: &BTreeMap<String, FixtureJsonValue>,
    key: &str,
) -> Result<Vec<String>, AppError> {
    let Some(FixtureJsonValue::StringArray(values)) = fields.get(key) else {
        return Err(AppError::usage(format!(
            "benchmark fixture에 필수 string array field가 없거나 type이 다릅니다: {key}"
        )));
    };
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(AppError::usage(format!(
            "benchmark fixture array field에는 빈 문자열을 넣을 수 없습니다: {key}"
        )));
    }
    Ok(values.clone())
}

fn validate_fixture_schema(fields: &BTreeMap<String, FixtureJsonValue>) -> Result<(), AppError> {
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

fn parse_fixture_json_object(text: &str) -> Result<BTreeMap<String, FixtureJsonValue>, AppError> {
    let mut rest = skip_ws(text);
    rest = rest.strip_prefix('{').ok_or_else(fixture_json_error)?;
    let mut fields = BTreeMap::new();
    rest = skip_ws(rest);
    if let Some(after_object) = rest.strip_prefix('}') {
        if skip_ws(after_object).is_empty() {
            return Ok(fields);
        }
        return Err(fixture_json_error());
    }

    loop {
        let (key, after_key) = parse_json_string_value(rest).ok_or_else(fixture_json_error)?;
        rest = skip_ws(after_key);
        rest = rest.strip_prefix(':').ok_or_else(fixture_json_error)?;
        rest = skip_ws(rest);
        let (value, after_value) = parse_fixture_json_value(rest)?;
        if fields.insert(key.clone(), value).is_some() {
            return Err(AppError::usage(format!(
                "benchmark fixture field가 중복되었습니다: {key}"
            )));
        }

        rest = skip_ws(after_value);
        if let Some(after_comma) = rest.strip_prefix(',') {
            rest = skip_ws(after_comma);
            if rest.starts_with('}') {
                return Err(fixture_json_error());
            }
            continue;
        }
        if let Some(after_object) = rest.strip_prefix('}') {
            if skip_ws(after_object).is_empty() {
                return Ok(fields);
            }
            return Err(fixture_json_error());
        }
        return Err(fixture_json_error());
    }
}

fn parse_fixture_json_value(text: &str) -> Result<(FixtureJsonValue, &str), AppError> {
    if text.starts_with('"') {
        let (value, rest) = parse_json_string_value(text).ok_or_else(fixture_json_error)?;
        return Ok((FixtureJsonValue::String(value), rest));
    }
    if text.starts_with('[') {
        let (value, rest) = parse_json_string_array_value(text)?;
        return Ok((FixtureJsonValue::StringArray(value), rest));
    }
    if let Some(rest) = text.strip_prefix("true") {
        return Ok((FixtureJsonValue::Bool(true), rest));
    }
    if let Some(rest) = text.strip_prefix("false") {
        return Ok((FixtureJsonValue::Bool(false), rest));
    }
    if text.starts_with(|ch: char| ch.is_ascii_digit()) {
        let (value, rest) = parse_json_u32_value(text).ok_or_else(fixture_json_error)?;
        return Ok((FixtureJsonValue::U32(value), rest));
    }
    Err(fixture_json_error())
}

fn parse_json_string_array_value(text: &str) -> Result<(Vec<String>, &str), AppError> {
    let mut rest = text.strip_prefix('[').ok_or_else(fixture_json_error)?;
    let mut values = Vec::new();
    rest = skip_ws(rest);
    if let Some(after_array) = rest.strip_prefix(']') {
        return Ok((values, after_array));
    }

    loop {
        let (value, after_string) = parse_json_string_value(rest).ok_or_else(fixture_json_error)?;
        values.push(value);
        rest = skip_ws(after_string);
        if let Some(after_comma) = rest.strip_prefix(',') {
            rest = skip_ws(after_comma);
            if rest.starts_with(']') {
                return Err(fixture_json_error());
            }
            continue;
        }
        if let Some(after_array) = rest.strip_prefix(']') {
            return Ok((values, after_array));
        }
        return Err(fixture_json_error());
    }
}

fn parse_json_string_value(text: &str) -> Option<(String, &str)> {
    let mut index = 0;
    let quote = text[index..].chars().next()?;
    if quote != '"' {
        return None;
    }
    index += quote.len_utf8();
    let mut value = String::new();

    while index < text.len() {
        let ch = text[index..].chars().next()?;
        index += ch.len_utf8();
        match ch {
            '"' => return Some((value, &text[index..])),
            '\\' => {
                let escaped = text[index..].chars().next()?;
                index += escaped.len_utf8();
                match escaped {
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    '/' => value.push('/'),
                    'b' => value.push('\u{0008}'),
                    'f' => value.push('\u{000C}'),
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    'u' => {
                        let (decoded, next_index) = parse_json_unicode_escape(text, index)?;
                        value.push(decoded);
                        index = next_index;
                    }
                    _ => return None,
                }
            }
            ch if ch <= '\u{001F}' => return None,
            other => value.push(other),
        }
    }

    None
}

fn parse_json_unicode_escape(text: &str, index: usize) -> Option<(char, usize)> {
    let (high, mut next_index) = parse_hex_quad(text, index)?;
    if (0xD800..=0xDBFF).contains(&high) {
        let slash = text[next_index..].chars().next()?;
        next_index += slash.len_utf8();
        let u = text[next_index..].chars().next()?;
        next_index += u.len_utf8();
        if slash != '\\' || u != 'u' {
            return None;
        }
        let (low, after_low) = parse_hex_quad(text, next_index)?;
        if !(0xDC00..=0xDFFF).contains(&low) {
            return None;
        }
        let scalar = 0x10000 + (((high - 0xD800) << 10) | (low - 0xDC00));
        return char::from_u32(scalar).map(|ch| (ch, after_low));
    }
    if (0xDC00..=0xDFFF).contains(&high) {
        return None;
    }
    char::from_u32(high).map(|ch| (ch, next_index))
}

fn parse_hex_quad(text: &str, index: usize) -> Option<(u32, usize)> {
    let mut value = 0_u32;
    let mut next_index = index;
    for _ in 0..4 {
        let ch = text[next_index..].chars().next()?;
        let digit = ch.to_digit(16)?;
        value = (value << 4) | digit;
        next_index += ch.len_utf8();
    }
    Some((value, next_index))
}

fn parse_json_u32_value(text: &str) -> Option<(u32, &str)> {
    let digits = text
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    let value = digits.parse().ok()?;
    Some((value, &text[digits.len()..]))
}

fn skip_ws(text: &str) -> &str {
    text.trim_start_matches(|ch: char| ch.is_ascii_whitespace())
}

fn fixture_json_error() -> AppError {
    AppError::usage("benchmark fixture JSON object가 schema parser를 통과하지 못했습니다.")
}

fn json_option(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|value| format!("\"{}\"", ledger::json_string(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn json_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", ledger::json_string(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn json_raw_or_string(value: &str) -> String {
    let trimmed = value.trim();
    if (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
    {
        trimmed.to_string()
    } else {
        format!("\"{}\"", ledger::json_string(value))
    }
}

fn harness_ref() -> String {
    format!("rpotato-benchmark-harness@{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_fixture_metadata() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-benchmark-validate-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        let fixture_path = write_fixture(&root);

        let report = validate_report(fixture_path.to_str().unwrap()).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("status: valid"));
        assert!(report.contains("fixture id: sample-fixture"));
        assert!(report.contains("raw prompt/source 저장: 없음"));
    }

    #[test]
    fn records_fixture_without_score_claim() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-benchmark-record-test-{}",
            std::process::id()
        ));
        let data_root = root.join("data");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_DATA_HOME", &data_root);
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        let fixture_path = write_fixture(&project_root);

        let report = record_report(fixture_path.to_str().unwrap()).unwrap();
        let export = report_export(BenchmarkReportFormat::Jsonl).unwrap();

        std::env::remove_var("RPOTATO_DATA_HOME");
        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("claim state: not-comparable"));
        assert!(report.contains("score: 없음"));
        assert!(export.contains("\"fixture_id\":\"sample-fixture\""));
        assert!(export.contains("\"claim_state\":\"not-comparable\""));
        assert!(export.contains("\"score\":null"));
    }

    #[test]
    fn rejects_array_trailing_comma() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-benchmark-invalid-array-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        let fixture_path = write_fixture(&root);
        let text = fs::read_to_string(&fixture_path)
            .unwrap()
            .replace("\"required_tools\": [],", "\"required_tools\": [\"rg\",],");
        fs::write(&fixture_path, text).unwrap();

        let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 2);
        assert!(err.message.contains("schema parser"));
    }

    #[test]
    fn rejects_malformed_context_budget_suffix() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-benchmark-invalid-number-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        let fixture_path = write_fixture(&root);
        mutate_fixture(&fixture_path, |text| {
            text.replace("\"context_budget\": 2048,", "\"context_budget\": 2048.5,")
        });

        let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 2);
        assert!(err.message.contains("schema parser"));
    }

    #[test]
    fn rejects_malformed_bool_suffix() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-benchmark-invalid-bool-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        let fixture_path = write_fixture(&root);
        mutate_fixture(&fixture_path, |text| {
            text.replace(
                "\"abstention_required\": false,",
                "\"abstention_required\": falsex,",
            )
        });

        let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 2);
        assert!(err.message.contains("schema parser"));
    }

    #[test]
    fn rejects_raw_prompt_field() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-benchmark-raw-prompt-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        let fixture_path = write_fixture(&root);
        mutate_fixture(&fixture_path, |text| {
            text.replace(
                "\"fixture_id\": \"sample-fixture\",",
                "\"fixture_id\": \"sample-fixture\",\n  \"raw_prompt\": \"secret\",",
            )
        });

        let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 2);
        assert!(err.message.contains("raw prompt/source field"));
    }

    #[test]
    fn rejects_duplicate_field() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rpotato-benchmark-duplicate-field-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &root);
        let fixture_path = write_fixture(&root);
        mutate_fixture(&fixture_path, |text| {
            text.replace(
                "\"fixture_id\": \"sample-fixture\",",
                "\"fixture_id\": \"sample-fixture\",\n  \"fixture_id\": \"duplicate\",",
            )
        });

        let err = validate_report(fixture_path.to_str().unwrap()).unwrap_err();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 2);
        assert!(err.message.contains("중복"));
    }

    fn write_fixture(root: &Path) -> PathBuf {
        let fixture = root.join("sample-fixture.json");
        fs::write(
            &fixture,
            r#"{
  "fixture_id": "sample-fixture",
  "benchmark_name": "foundation-smoke",
  "runtime_capability_under_test": "fixture-validation",
  "model_vs_runtime_responsibility": "runtime validates metadata; model is not executed",
  "expected_route": "benchmark.record",
  "expected_policy_decision": "allow",
  "expected_escalation_target": "none",
  "required_tools": [],
  "required_source_reads": [],
  "required_evidence_records": ["benchmark_run"],
  "abstention_required": false,
  "expected_failure_category": "none",
  "ontology_view": "static-summary",
  "context_budget": 2048,
  "model_id": "not-applicable",
  "model_artifact_hash": "not-applicable",
  "quantization": "not-applicable",
  "backend_id": "not-applicable",
  "backend_version": "not-applicable",
  "dataset_ref": "local-fixture",
  "prompt_runtime_version": "rpotato-test",
  "tool_policy_version": "rpotato-test",
  "seed_policy": "fixed-0",
  "sampling_options": "not-applicable",
  "raw_artifact_retention_policy": "redacted-only"
}"#,
        )
        .unwrap();
        fixture
    }

    fn mutate_fixture(path: &Path, mutate: impl FnOnce(String) -> String) {
        let text = fs::read_to_string(path).unwrap();
        fs::write(path, mutate(text)).unwrap();
    }
}
