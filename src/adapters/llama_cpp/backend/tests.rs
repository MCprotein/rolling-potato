use super::*;
use crate::runtime_core::inference::backend::{
    BackendChatImage, BackendChatInput, BackendChatSampling, BackendResponseFormat,
};
use std::path::Path;

#[test]
fn health_status_line_does_not_require_connection_eof() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: keep-alive\r\n";

    assert_eq!(
        first_http_status_line(response).as_deref(),
        Some("HTTP/1.1 200 OK")
    );
    assert_eq!(first_http_status_line(b"HTTP/1.1 200 OK"), None);
}

#[test]
fn chat_request_disables_qwen_thinking_and_enables_usage_stream() {
    let body = chat_request_body(
        Path::new("Qwen3.5-4B-Q4_K_M.gguf"),
        "감자는 무엇인가?",
        64,
        &BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        true,
    );

    assert!(body.contains("\"chat_template_kwargs\":{\"enable_thinking\":false}"));
    assert!(body.contains("\"max_tokens\":64"));
    assert!(body.contains("\"stream\":true"));
    assert!(body.contains("\"include_usage\":true"));
    assert!(body.contains("reasoning trace"));
    assert!(body.contains("감자는 무엇인가?"));
}

#[test]
fn chat_request_disables_gemma_4_thinking() {
    let body = chat_request_body(
        Path::new("gemma-4-E4B_q4_0-it.gguf"),
        "감자",
        64,
        &BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        true,
    );

    assert!(body.contains("\"chat_template_kwargs\":{\"enable_thinking\":false}"));
    assert!(body.contains("\"temperature\":0.1"));
}

#[test]
fn chat_request_omits_thinking_option_for_unrecognized_models() {
    let body = chat_request_body(
        Path::new("custom-model.gguf"),
        "감자",
        64,
        &BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        true,
    );

    assert!(!body.contains("chat_template_kwargs"));
}

#[test]
fn multimodal_request_uses_openai_image_content_parts() {
    let input = BackendChatInput {
        text: "이 이미지의 오류를 설명해줘".to_string(),
        images: vec![BackendChatImage {
            display_name: "screen.png".to_string(),
            mime_type: "image/png".to_string(),
            sha256: "a".repeat(64),
            bytes: b"abc".to_vec(),
        }],
        response_language: crate::runtime_core::inference::backend::ResponseLanguage::KoreanDefault,
        response_format: BackendResponseFormat::Text,
    };

    let body = chat_request_body_for_input(
        Path::new("gemma-4-E4B_q4_0-it.gguf"),
        &input,
        64,
        &BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        true,
    )
    .unwrap();

    assert!(body.contains("\"type\":\"text\""));
    assert!(body.contains("\"type\":\"image_url\""));
    assert!(body.contains("data:image/png;base64,YWJj"));
    assert!(!body.contains("screen.png"));
}

#[test]
fn structured_chat_request_constrains_the_model_to_the_runtime_schema() {
    let input = BackendChatInput::text_for_user("도구를 선택해", "최신 Rust를 검색해줘")
        .with_json_schema(
            r#"{"type":"object","properties":{"decision":{"type":"string"}},"required":["decision"],"additionalProperties":false}"#,
        );

    let body = chat_request_body_for_input(
        Path::new("qwen3.5-4b.gguf"),
        &input,
        64,
        &BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        true,
    )
    .unwrap();

    assert!(body.contains("\"response_format\":{\"type\":\"json_object\",\"schema\":{"));
    assert!(body.contains("\"required\":[\"decision\"]"));
    assert!(body.contains("\"stream\":true"));
}

#[test]
fn production_turn_schema_stays_within_managed_grammar_limit() {
    let input = BackendChatInput::text("도구를 선택해")
        .with_json_schema(crate::runtime_core::agent::TURN_DECISION_JSON_SCHEMA);
    let body = chat_request_body_for_input(
        Path::new("qwen3.5-4b.gguf"),
        &input,
        512,
        &BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        true,
    )
    .unwrap();

    assert!(body.contains(r#""answer":{"type":"string"}"#));
    assert!(!body.contains(r#""answer":{"type":"string","maxLength":"#));
}

#[test]
fn rejects_schema_repetitions_that_managed_llama_cannot_compile() {
    for key in JSON_SCHEMA_REPETITION_KEYS {
        let accepted = BackendChatInput::text("도구를 선택해").with_json_schema(format!(
            r#"{{"type":"object","properties":{{"value":{{"type":"string","{key}":1999}}}}}}"#
        ));
        chat_request_body_for_input(
            Path::new("qwen3.5-4b.gguf"),
            &accepted,
            64,
            &BackendChatSampling {
                temperature: 0.1,
                top_p: 0.8,
            },
            true,
        )
        .unwrap();

        let rejected = BackendChatInput::text("도구를 선택해").with_json_schema(format!(
            r#"{{"type":"object","properties":{{"value":{{"type":"string","{key}":2000}}}}}}"#
        ));
        let error = chat_request_body_for_input(
            Path::new("qwen3.5-4b.gguf"),
            &rejected,
            64,
            &BackendChatSampling {
                temperature: 0.1,
                top_p: 0.8,
            },
            true,
        )
        .unwrap_err();

        assert!(error.message.contains(key), "{key}: {}", error.message);
        assert!(error.message.contains("1999"), "{key}: {}", error.message);
    }
}

#[test]
fn allows_schema_property_names_that_match_repetition_keywords() {
    let input = BackendChatInput::text("속성을 채워")
        .with_json_schema(
            r#"{"type":"object","properties":{"maxLength":{"type":"string"},"nested":{"type":"object","properties":{"minItems":{"type":"integer"}}}}}"#,
        );

    let body = chat_request_body_for_input(
        Path::new("qwen3.5-4b.gguf"),
        &input,
        64,
        &BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        true,
    )
    .unwrap();

    assert!(body.contains(r#""maxLength":{"type":"string"}"#));
    assert!(body.contains(r#""minItems":{"type":"integer"}"#));
}

#[test]
fn base64_encoder_matches_rfc_4648_padding_vectors() {
    assert_eq!(encode_base64(b""), "");
    assert_eq!(encode_base64(b"f"), "Zg==");
    assert_eq!(encode_base64(b"fo"), "Zm8=");
    assert_eq!(encode_base64(b"foo"), "Zm9v");
    assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
}

#[test]
fn request_system_policy_respects_an_explicit_output_language() {
    let body = chat_request_body(
        Path::new("model.gguf"),
        "이 문장을 영어로 번역해줘",
        32,
        &BackendChatSampling {
            temperature: 0.1,
            top_p: 0.8,
        },
        false,
    );

    assert!(body.contains("명시적으로 요청한 출력 언어"));
    assert!(!body.contains("기본 답변은 자연스러운 한국어"));
}

#[test]
fn vision_ready_sidecar_enters_llama_server_with_mmproj() {
    let command = sidecar_command(
        Path::new("/bin/llama-server"),
        Path::new("/models/model.gguf"),
        Some(Path::new("/models/mmproj.gguf")),
        "127.0.0.1",
        17842,
        Some(4096),
    );
    let args = command
        .get_args()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        args,
        [
            "--model",
            "/models/model.gguf",
            "--host",
            "127.0.0.1",
            "--port",
            "17842",
            "--mmproj",
            "/models/mmproj.gguf",
            "--ctx-size",
            "4096"
        ]
    );
}

#[test]
fn text_ready_sidecar_does_not_claim_mmproj() {
    let command = sidecar_command(
        Path::new("/bin/llama-server"),
        Path::new("/models/model.gguf"),
        None,
        "127.0.0.1",
        17842,
        None,
    );
    let args = command
        .get_args()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(!args.iter().any(|value| value == "--mmproj"));
}
