//! Live grammar/parser compatibility probe for the managed llama.cpp revision.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use super::*;

#[test]
#[ignore = "requires the pinned managed llama.cpp server and checksummed tiny model"]
fn managed_llama_parser_accepts_local_turn_schema() {
    let host =
        std::env::var("RPOTATO_LLAMA_PARSER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("RPOTATO_LLAMA_PARSER_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let input = BackendChatInput::text("프로젝트 파일을 확인해")
        .with_json_schema(crate::runtime_core::agent::LOCAL_TURN_DECISION_JSON_SCHEMA);
    let body = chat_request_body_for_input(&input, 1, &runtime_profile(false), false).unwrap();

    assert!(body.contains("\"max_tokens\":1"));
    assert!(body.contains("read_file"));
    assert!(!body.contains("web_search"));

    let mut stream = TcpStream::connect((host.as_str(), port))
        .expect("pinned managed llama-server must accept parser probe connections");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    write!(
        stream,
        "POST /v1/chat/completions HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let status = first_http_status_line(&response).unwrap_or_else(|| {
        String::from_utf8_lossy(&response)
            .chars()
            .take(512)
            .collect()
    });
    assert!(
        status.starts_with("HTTP/1.1 2") || status.starts_with("HTTP/1.0 2"),
        "llama.cpp rejected the production local-turn schema: {status}"
    );
}
