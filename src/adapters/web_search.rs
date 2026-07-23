//! Read-only web search through Exa's hosted Streamable HTTP MCP endpoint.

use std::time::Duration;

use crate::foundation::error::AppError;
use crate::foundation::serialization::{self, Value};

const EXA_MCP_ENDPOINT: &str = "https://mcp.exa.ai/mcp";
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_MCP_RESPONSE_BYTES: u64 = 512 * 1024;
const MAX_SEARCH_CONTEXT_CHARS: usize = 6 * 1024;
const MAX_SOURCES: usize = 4;
const MAX_SOURCE_URL_BYTES: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebSearchEvidence {
    pub(crate) context: String,
    pub(crate) sources: Vec<String>,
}

pub(crate) fn search(query: &str) -> Result<WebSearchEvidence, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::usage("웹 검색어가 필요합니다."));
    }

    #[cfg(debug_assertions)]
    if let Some(fixture) = std::env::var_os("RPOTATO_TEST_WEB_SEARCH_SSE") {
        return parse_tool_response(&fixture.to_string_lossy());
    }

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .https_only(true)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let initialize = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{{\"protocolVersion\":\"{MCP_PROTOCOL_VERSION}\",\"capabilities\":{{}},\"clientInfo\":{{\"name\":\"rpotato\",\"version\":\"{}\"}}}}}}",
        env!("CARGO_PKG_VERSION")
    );
    let mut response = post_mcp(&agent, "initialize", None, None, &initialize)?;
    let session_id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| AppError::runtime("웹 검색 MCP가 session id를 반환하지 않았습니다."))?;
    read_bounded_body(&mut response, "웹 검색 MCP 초기화")?;

    let initialized = "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}";
    let mut response = post_mcp(
        &agent,
        "notifications/initialized",
        None,
        Some(&session_id),
        initialized,
    )?;
    read_bounded_body(&mut response, "웹 검색 MCP 초기화 확인")?;

    let escaped_query = serialization::escape_string_content(query);
    let request = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{{\"name\":\"web_search_exa\",\"arguments\":{{\"query\":\"{escaped_query}\",\"numResults\":5}}}}}}"
    );
    let mut response = post_mcp(
        &agent,
        "tools/call",
        Some("web_search_exa"),
        Some(&session_id),
        &request,
    )?;
    let body = read_bounded_body(&mut response, "웹 검색 결과")?;
    parse_tool_response(&body)
}

fn post_mcp(
    agent: &ureq::Agent,
    method: &str,
    name: Option<&str>,
    session_id: Option<&str>,
    body: &str,
) -> Result<ureq::http::Response<ureq::Body>, AppError> {
    let mut request = agent
        .post(EXA_MCP_ENDPOINT)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", MCP_PROTOCOL_VERSION)
        .header("Mcp-Method", method)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")));
    if let Some(name) = name {
        request = request.header("Mcp-Name", name);
    }
    if let Some(session_id) = session_id {
        request = request.header("Mcp-Session-Id", session_id);
    }
    request
        .send(body.as_bytes())
        .map_err(|error| AppError::runtime(format!("웹 검색 연결 실패: {error}")))
}

fn read_bounded_body(
    response: &mut ureq::http::Response<ureq::Body>,
    context: &str,
) -> Result<String, AppError> {
    response
        .body_mut()
        .with_config()
        .limit(MAX_MCP_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|error| AppError::runtime(format!("{context} 읽기 실패: {error}")))
}

fn parse_tool_response(body: &str) -> Result<WebSearchEvidence, AppError> {
    let sse_payloads = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let payload = sse_payloads
        .iter()
        .rev()
        .find(|payload| {
            matches!(
                serialization::parse_value(payload, "Exa MCP SSE event"),
                Ok(Value::Object(ref root))
                    if root.contains_key("result") || root.contains_key("error")
            )
        })
        .copied()
        .unwrap_or_else(|| body.trim());
    let Value::Object(root) = serialization::parse_value(payload, "Exa MCP tools/call")? else {
        return Err(AppError::blocked(
            "웹 검색 응답 root 형식이 올바르지 않습니다.",
        ));
    };
    if let Some(Value::Object(error)) = root.get("error") {
        let message = match error.get("message") {
            Some(Value::String(message)) => message.as_str(),
            _ => "알 수 없는 MCP 오류",
        };
        return Err(AppError::runtime(format!("웹 검색 도구 오류: {message}")));
    }
    let Some(Value::Object(result)) = root.get("result") else {
        return Err(AppError::blocked("웹 검색 응답에 result가 없습니다."));
    };
    if matches!(result.get("isError"), Some(Value::Bool(true))) {
        return Err(AppError::runtime(
            "웹 검색 제공자가 요청을 처리하지 못했습니다.",
        ));
    }
    let Some(Value::Array(content)) = result.get("content") else {
        return Err(AppError::blocked("웹 검색 응답에 content가 없습니다."));
    };
    let text = content
        .iter()
        .filter_map(|item| match item {
            Value::Object(item) => match (item.get("type"), item.get("text")) {
                (Some(Value::String(kind)), Some(Value::String(text))) if kind == "text" => {
                    Some(text.as_str())
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() {
        return Err(AppError::blocked("웹 검색 결과가 비어 있습니다."));
    }
    let context = text
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(MAX_SEARCH_CONTEXT_CHARS)
        .collect::<String>();
    let mut sources = Vec::new();
    for url in context
        .lines()
        .filter_map(|line| line.trim().strip_prefix("URL: "))
    {
        let url = normalize_source_url(url);
        if is_valid_https_source_url(&url) && !sources.iter().any(|stored| stored == &url) {
            sources.push(url);
            if sources.len() == MAX_SOURCES {
                break;
            }
        }
    }
    if sources.is_empty() {
        return Err(AppError::blocked(
            "웹 검색 결과에 검증 가능한 HTTPS 출처가 없습니다.",
        ));
    }
    Ok(WebSearchEvidence { context, sources })
}

fn normalize_source_url(url: &str) -> String {
    let Some((scheme, remainder)) = url.split_once("://") else {
        return url.to_string();
    };
    if !remainder.ends_with("//") {
        return url.to_string();
    }
    format!("{scheme}://{}/", remainder.trim_end_matches('/'))
}

fn is_valid_https_source_url(url: &str) -> bool {
    if url.len() > MAX_SOURCE_URL_BYTES
        || url
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return false;
    }
    let Ok(uri) = url.parse::<ureq::http::Uri>() else {
        return false;
    };
    uri.scheme_str() == Some("https")
        && uri.authority().is_some_and(|authority| {
            !authority.host().is_empty() && !authority.as_str().contains('@')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_text_and_https_sources_from_sse() {
        let body = r#"event: message
data: {"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":1}}

event: message
data: {"result":{"content":[{"type":"text","text":"Title: 공식 문서\nURL: https://example.com/docs\nHighlights:\n최신 정보입니다.\n\n---\n\nTitle: 중복\nURL: https://example.com/docs"}],"isError":false},"jsonrpc":"2.0","id":2}
"#;

        let evidence = parse_tool_response(body).unwrap();

        assert!(evidence.context.contains("최신 정보입니다."));
        assert_eq!(evidence.sources, vec!["https://example.com/docs"]);
    }

    #[test]
    fn normalizes_duplicate_trailing_slashes_in_source_urls() {
        let body = r#"data: {"result":{"content":[{"type":"text","text":"Title: Rust\nURL: https://rust-lang.org//"}],"isError":false},"jsonrpc":"2.0","id":2}"#;

        let evidence = parse_tool_response(body).unwrap();

        assert_eq!(evidence.sources, vec!["https://rust-lang.org/"]);
    }

    #[test]
    fn bounds_small_model_context_and_only_exposes_sources_inside_it() {
        let text = format!(
            "Title: 첫 결과\nURL: https://example.com/first\nHighlights:\n{}\nTitle: 잘린 결과\nURL: https://example.com/truncated",
            "가".repeat(MAX_SEARCH_CONTEXT_CHARS)
        );
        let body = format!(
            "data: {{\"result\":{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}],\"isError\":false}},\"jsonrpc\":\"2.0\",\"id\":2}}",
            serialization::escape_string_content(&text)
        );

        let evidence = parse_tool_response(&body).unwrap();

        assert!(evidence.context.chars().count() <= MAX_SEARCH_CONTEXT_CHARS);
        assert_eq!(evidence.sources, vec!["https://example.com/first"]);
    }

    #[test]
    fn rejects_tool_errors_and_results_without_sources() {
        let tool_error = r#"data: {"result":{"content":[],"isError":true},"jsonrpc":"2.0","id":2}"#;
        assert!(parse_tool_response(tool_error).is_err());

        let no_source = r#"data: {"result":{"content":[{"type":"text","text":"출처 없음"}]},"jsonrpc":"2.0","id":2}"#;
        assert!(parse_tool_response(no_source).is_err());
    }

    #[test]
    fn rejects_malformed_or_deceptive_https_sources() {
        let malformed = [
            "https://",
            "https://user@example.com/docs",
            "https://example.com/a path",
            "https://example.com/\nforged",
        ];
        for url in malformed {
            assert!(!is_valid_https_source_url(url), "url: {url}");
        }
        for url in &malformed[..3] {
            let body = format!(
                "data: {{\"result\":{{\"content\":[{{\"type\":\"text\",\"text\":\"Title: forged\\nURL: {}\"}}],\"isError\":false}},\"jsonrpc\":\"2.0\",\"id\":2}}",
                serialization::escape_string_content(url)
            );

            assert!(parse_tool_response(&body).is_err(), "url: {url}");
        }
        assert!(is_valid_https_source_url("https://example.com/docs?q=rust"));
    }

    #[test]
    fn live_web_search_smoke_when_explicitly_enabled() {
        if std::env::var("RPOTATO_RUN_LIVE_WEB_SEARCH").as_deref() != Ok("1") {
            return;
        }

        let evidence = search("Rust 공식 웹사이트 프로그래밍 언어").unwrap();

        assert!(!evidence.context.trim().is_empty());
        assert!(evidence
            .sources
            .iter()
            .all(|source| source.starts_with("https://")));
    }
}
