//! OpenAI-compatible chat request serialization.

use crate::foundation::error::AppError;
use crate::foundation::serialization::{self, escape_string_content, Value};
#[cfg(test)]
use crate::runtime_core::inference::backend::BackendChatSampling;
use crate::runtime_core::inference::backend::{
    BackendChatInput, BackendChatRuntimeProfile, BackendResponseFormat,
};

const MAX_JSON_SCHEMA_REPETITION: u128 = 1_999;
pub(super) const JSON_SCHEMA_REPETITION_KEYS: [&str; 6] = [
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
    "minProperties",
    "maxProperties",
];

#[cfg(test)]
pub(crate) fn chat_request_body(
    prompt: &str,
    max_tokens: u32,
    sampling: &BackendChatSampling,
    disable_thinking_via_template: bool,
    stream: bool,
) -> String {
    let runtime_profile = BackendChatRuntimeProfile {
        sampling_profile_version: "request-test-v1".to_string(),
        sampling: Some(*sampling),
        disable_thinking_via_template,
        thinking_mode: if disable_thinking_via_template {
            "test-disabled".to_string()
        } else {
            "test-model-default".to_string()
        },
        thinking_source: "test-source".to_string(),
    };
    chat_request_body_for_input(
        &BackendChatInput::text_for_user(prompt, prompt),
        max_tokens,
        &runtime_profile,
        stream,
    )
    .expect("plain text chat request must be serializable")
}

pub(crate) fn chat_request_body_for_input(
    input: &BackendChatInput,
    max_tokens: u32,
    runtime_profile: &BackendChatRuntimeProfile,
    stream: bool,
) -> Result<String, AppError> {
    validate_response_schema(input)?;
    let system_prompt = if input.response_language.allows_non_korean() {
        "사용자가 명시적으로 요청한 출력 언어를 따릅니다. reasoning trace, <think> 태그, 내부 추론은 출력하지 않습니다."
    } else {
        "기본 답변은 자연스러운 한국어로 작성하고, 코드·수식·URL·고유명사는 필요한 원문 표기를 유지합니다. reasoning trace, <think> 태그, 내부 추론은 출력하지 않습니다."
    };
    let template_options = if runtime_profile.disable_thinking_via_template {
        ",\"chat_template_kwargs\":{\"enable_thinking\":false}"
    } else {
        ""
    };
    let stream_options = if stream {
        ",\"stream\":true,\"stream_options\":{\"include_usage\":true}"
    } else {
        ""
    };
    let response_format = match &input.response_format {
        BackendResponseFormat::Text => String::new(),
        BackendResponseFormat::JsonSchema { schema } => {
            format!(",\"response_format\":{{\"type\":\"json_object\",\"schema\":{schema}}}")
        }
    };
    let sampling_options = runtime_profile
        .sampling
        .map(|sampling| {
            format!(
                ",\"temperature\":{},\"top_p\":{}",
                sampling.temperature, sampling.top_p
            )
        })
        .unwrap_or_default();
    let user_content = if input.images.is_empty() {
        format!("\"{}\"", escape_string_content(&input.text))
    } else {
        let mut parts = Vec::with_capacity(input.images.len() + 1);
        if !input.text.trim().is_empty() {
            parts.push(format!(
                "{{\"type\":\"text\",\"text\":\"{}\"}}",
                escape_string_content(&input.text)
            ));
        }
        parts.extend(input.images.iter().map(|image| {
            format!(
                "{{\"type\":\"image_url\",\"image_url\":{{\"url\":\"data:{};base64,{}\"}}}}",
                escape_string_content(&image.mime_type),
                encode_base64(&image.bytes)
            )
        }));
        format!("[{}]", parts.join(","))
    };
    Ok(format!(
        "{{\"messages\":[{{\"role\":\"system\",\"content\":\"{}\"}},{{\"role\":\"user\",\"content\":{}}}],\"max_tokens\":{}{}{}{}{}}}",
        escape_string_content(system_prompt),
        user_content,
        max_tokens,
        sampling_options,
        template_options,
        response_format,
        stream_options
    ))
}

fn validate_response_schema(input: &BackendChatInput) -> Result<(), AppError> {
    let BackendResponseFormat::JsonSchema { schema } = &input.response_format else {
        return Ok(());
    };
    let schema = serialization::parse_value(schema, "llama.cpp JSON response schema")?;
    validate_schema_repetitions(&schema)
}

fn validate_schema_repetitions(value: &Value) -> Result<(), AppError> {
    let Value::Object(object) = value else {
        return Ok(());
    };

    for key in JSON_SCHEMA_REPETITION_KEYS {
        let Some(repetition) = object.get(key) else {
            continue;
        };
        let Value::Number(repetition) = repetition else {
            return Err(AppError::blocked(format!(
                "llama.cpp JSON schema 차단\n- 이유: {key}는 음수가 아닌 정수여야 합니다."
            )));
        };
        if *repetition > MAX_JSON_SCHEMA_REPETITION {
            return Err(AppError::blocked(format!(
                "llama.cpp JSON schema 차단\n- 이유: {key}={repetition}은 managed grammar 상한 {MAX_JSON_SCHEMA_REPETITION}을 초과합니다."
            )));
        }
    }

    for key in [
        "additionalProperties",
        "contains",
        "contentSchema",
        "else",
        "if",
        "items",
        "not",
        "propertyNames",
        "then",
        "unevaluatedItems",
        "unevaluatedProperties",
    ] {
        if let Some(schema) = object.get(key) {
            validate_schema_repetitions(schema)?;
        }
    }

    for key in [
        "$defs",
        "definitions",
        "dependentSchemas",
        "patternProperties",
        "properties",
    ] {
        if let Some(Value::Object(schemas)) = object.get(key) {
            for schema_name in schemas.keys() {
                let schema = schemas
                    .get(schema_name)
                    .expect("JSON object key iterator and lookup must agree");
                validate_schema_repetitions(schema)?;
            }
        }
    }

    for key in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(Value::Array(values)) = object.get(key) {
            for nested in values {
                validate_schema_repetitions(nested)?;
            }
        }
    }

    Ok(())
}

pub(super) fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(third & 0b11_1111) as usize] as char
        } else {
            '='
        });
    }
    encoded
}
