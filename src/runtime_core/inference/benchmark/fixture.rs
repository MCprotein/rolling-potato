use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::foundation::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BenchmarkFixture {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
    pub(crate) fixture_id: String,
    pub(crate) benchmark_name: String,
    pub(crate) runtime_capability_under_test: String,
    pub(crate) model_vs_runtime_responsibility: String,
    pub(crate) expected_route: String,
    pub(crate) expected_policy_decision: String,
    pub(crate) expected_escalation_target: String,
    pub(crate) required_tools: Vec<String>,
    pub(crate) required_source_reads: Vec<String>,
    pub(crate) required_evidence_records: Vec<String>,
    pub(crate) abstention_required: bool,
    pub(crate) expected_failure_category: String,
    pub(crate) ontology_view: String,
    pub(crate) context_budget: u32,
    pub(crate) model_id: String,
    pub(crate) model_artifact_hash: String,
    pub(crate) quantization: String,
    pub(crate) backend_id: String,
    pub(crate) backend_version: String,
    pub(crate) dataset_ref: String,
    pub(crate) prompt_runtime_version: String,
    pub(crate) tool_policy_version: String,
    pub(crate) seed_policy: String,
    pub(crate) sampling_options: String,
    pub(crate) raw_artifact_retention_policy: String,
    pub(crate) expected_response_contains: Vec<String>,
    pub(crate) forbidden_response_contains: Vec<String>,
    pub(crate) minimum_score: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BenchmarkPromptArtifact {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
    pub(crate) text: String,
    pub(crate) chars: u32,
}

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

    if fixture.minimum_score.is_some_and(|score| score > 3) {
        return Err(AppError::usage("minimum_score는 0부터 3 사이여야 합니다."));
    }

    Ok(())
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

fn optional_string_array(
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
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(AppError::usage(format!(
            "benchmark fixture array field에는 빈 문자열을 넣을 수 없습니다: {key}"
        )));
    }
    Ok(values.clone())
}

fn optional_u32(
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
