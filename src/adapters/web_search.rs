//! Bounded read-only web search through Brave's direct REST API.

use std::time::Duration;

use crate::foundation::error::AppError;
use crate::foundation::serialization::{self, Object, Value};

const BRAVE_WEB_SEARCH_ENDPOINT: &str = "https://api.search.brave.com/res/v1/web/search";
const MAX_SEARCH_RESPONSE_BYTES: u64 = 512 * 1024;
const MAX_SEARCH_CONTEXT_CHARS: usize = 6 * 1024;
const MAX_QUERY_CHARS: usize = 400;
const MAX_QUERY_WORDS: usize = 50;
const MAX_SOURCES: usize = 4;
const MAX_SOURCE_URL_BYTES: usize = 2_048;
const RESULT_COUNT: &str = "5";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebSearchEvidence {
    pub(crate) context: String,
    pub(crate) sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SearchResult {
    title: String,
    url: String,
    description: String,
    extra_snippets: Vec<String>,
}

pub(crate) fn search(query: &str) -> Result<WebSearchEvidence, AppError> {
    let query = validate_query(query)?;

    #[cfg(debug_assertions)]
    if let Some(fixture) = std::env::var_os("RPOTATO_TEST_WEB_SEARCH_JSON") {
        return parse_search_response(&fixture.to_string_lossy());
    }

    let api_key = configured_api_key()?;
    let config = brave_agent_config();
    let agent = ureq::Agent::new_with_config(config);
    let mut response = agent
        .get(BRAVE_WEB_SEARCH_ENDPOINT)
        .query("q", query)
        .query("count", RESULT_COUNT)
        .query("country", "KR")
        .query("search_lang", "ko")
        .query("ui_lang", "ko-KR")
        .query("safesearch", "moderate")
        .query("extra_snippets", "true")
        .header("Accept", "application/json")
        .header("X-Subscription-Token", &api_key)
        .header("User-Agent", concat!("rpotato/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(map_search_error)?;
    if response.status().is_redirection() {
        return Err(AppError::blocked(
            "웹 검색 제공자가 redirect를 반환해 credential 보호를 위해 요청을 중단했습니다.",
        ));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_SEARCH_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|_| AppError::runtime("웹 검색 응답을 제한된 크기로 읽지 못했습니다."))?;
    parse_search_response(&body)
}

pub(crate) fn configuration_summary() -> String {
    let primary = non_empty_env("BRAVE_SEARCH_API_KEY");
    let alias = non_empty_env("BRAVE_API_KEY");
    configuration_summary_from(primary.as_deref(), alias.as_deref()).to_string()
}

fn configuration_summary_from(primary: Option<&str>, alias: Option<&str>) -> &'static str {
    match (primary, alias) {
        (Some(primary), Some(alias)) if primary != alias => {
            "설정 충돌; BRAVE_SEARCH_API_KEY와 BRAVE_API_KEY를 같은 값으로 맞추거나 하나만 사용"
        }
        (Some(_), _) | (_, Some(_)) => "사용 가능; Brave direct REST, environment-only key",
        (None, None) => "미설정; BRAVE_SEARCH_API_KEY 필요",
    }
}

fn brave_agent_config() -> ureq::config::Config {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .https_only(true)
        .max_redirects(0)
        .build()
}

fn validate_query(query: &str) -> Result<&str, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::usage("웹 검색어가 필요합니다."));
    }
    if query.chars().count() > MAX_QUERY_CHARS || query.split_whitespace().count() > MAX_QUERY_WORDS
    {
        return Err(AppError::usage(format!(
            "웹 검색어는 최대 {MAX_QUERY_CHARS}자, {MAX_QUERY_WORDS}단어까지 허용합니다."
        )));
    }
    if query
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\t' | '\n'))
    {
        return Err(AppError::usage(
            "웹 검색어에는 제어 문자를 사용할 수 없습니다.",
        ));
    }
    Ok(query)
}

fn configured_api_key() -> Result<String, AppError> {
    let primary = non_empty_env("BRAVE_SEARCH_API_KEY");
    let alias = non_empty_env("BRAVE_API_KEY");
    configured_api_key_from(primary.as_deref(), alias.as_deref())
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn configured_api_key_from(primary: Option<&str>, alias: Option<&str>) -> Result<String, AppError> {
    match (primary, alias) {
        (Some(primary), Some(alias)) if primary != alias => Err(AppError::usage(
            "Brave Search API key 환경변수가 서로 다릅니다. BRAVE_SEARCH_API_KEY와 BRAVE_API_KEY 중 하나만 설정하거나 같은 값으로 맞추세요.",
        )),
        (Some(value), _) | (_, Some(value)) => Ok(value.to_string()),
        (None, None) => Err(AppError::usage(
            "웹 검색을 사용하려면 Brave Search API key가 필요합니다.\n- 권장: BRAVE_SEARCH_API_KEY 환경변수를 설정하세요.\n- key는 rpotato 설정 파일이나 로그에 저장되지 않습니다.",
        )),
    }
}

fn map_search_error(error: ureq::Error) -> AppError {
    match error {
        ureq::Error::StatusCode(401 | 403) => {
            AppError::usage("Brave Search 인증에 실패했습니다. API key와 구독 상태를 확인하세요.")
        }
        ureq::Error::StatusCode(429) => {
            AppError::runtime("Brave Search 요청 한도에 도달했습니다. 잠시 뒤 다시 시도하세요.")
        }
        ureq::Error::StatusCode(400..=499) => {
            AppError::runtime("Brave Search가 요청을 거부했습니다.")
        }
        ureq::Error::StatusCode(500..=599) => {
            AppError::runtime("Brave Search 서비스가 일시적으로 응답하지 않습니다.")
        }
        _ => AppError::runtime("웹 검색 제공자에 연결하지 못했습니다."),
    }
}

fn parse_search_response(body: &str) -> Result<WebSearchEvidence, AppError> {
    let Value::Object(root) = serialization::parse_value(body, "Brave Search 응답")? else {
        return Err(AppError::blocked(
            "웹 검색 응답 root 형식이 올바르지 않습니다.",
        ));
    };
    let Some(Value::Object(web)) = root.get("web") else {
        return Err(AppError::blocked("웹 검색 응답에 web 결과가 없습니다."));
    };
    let Some(Value::Array(results)) = web.get("results") else {
        return Err(AppError::blocked("웹 검색 응답에 web.results가 없습니다."));
    };
    let mut parsed = Vec::new();
    for result in results.iter().filter_map(parse_result) {
        if !is_valid_https_source_url(&result.url)
            || parsed
                .iter()
                .any(|stored: &SearchResult| stored.url == result.url)
        {
            continue;
        }
        parsed.push(result);
        if parsed.len() == MAX_SOURCES {
            break;
        }
    }
    if parsed.is_empty() {
        return Err(AppError::blocked(
            "웹 검색 결과에 검증 가능한 HTTPS 출처가 없습니다.",
        ));
    }
    evidence_from_results(&parsed)
}

fn parse_result(value: &Value) -> Option<SearchResult> {
    let Value::Object(result) = value else {
        return None;
    };
    let title = string_field(result, "title")?;
    let url = string_field(result, "url")?;
    let description = string_field(result, "description").unwrap_or_default();
    let extra_snippets = match result.get("extra_snippets") {
        Some(Value::Array(snippets)) => snippets
            .iter()
            .filter_map(|snippet| match snippet {
                Value::String(snippet) => Some(snippet.clone()),
                _ => None,
            })
            .take(5)
            .collect(),
        _ => Vec::new(),
    };
    Some(SearchResult {
        title,
        url,
        description,
        extra_snippets,
    })
}

fn string_field(object: &Object, key: &str) -> Option<String> {
    match object.get(key) {
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}

fn evidence_from_results(results: &[SearchResult]) -> Result<WebSearchEvidence, AppError> {
    let mut context = String::new();
    let mut sources = Vec::new();
    for result in results {
        if sources.iter().any(|stored| stored == &result.url) {
            continue;
        }
        let mut section = format!(
            "Title: {}\nURL: {}\nDescription: {}",
            sanitize_context(&result.title),
            result.url,
            sanitize_context(&result.description)
        );
        for snippet in &result.extra_snippets {
            section.push_str("\nSnippet: ");
            section.push_str(&sanitize_context(snippet));
        }
        let separator = if context.is_empty() {
            ""
        } else {
            "\n\n---\n\n"
        };
        let remaining = MAX_SEARCH_CONTEXT_CHARS.saturating_sub(context.chars().count());
        if remaining <= separator.chars().count() {
            break;
        }
        let bounded_section = section
            .chars()
            .take(remaining - separator.chars().count())
            .collect::<String>();
        if bounded_section.trim().is_empty() {
            break;
        }
        context.push_str(separator);
        context.push_str(&bounded_section);
        sources.push(result.url.clone());
        if context.chars().count() == MAX_SEARCH_CONTEXT_CHARS {
            break;
        }
    }
    if sources.is_empty() {
        return Err(AppError::blocked(
            "웹 검색 결과가 작은 모델용 context 한도 안에 들어오지 않았습니다.",
        ));
    }
    Ok(WebSearchEvidence { context, sources })
}

fn sanitize_context(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .collect::<String>()
        .trim()
        .to_string()
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

    const FIXTURE: &str = r#"{
      "type":"search",
      "web":{
        "results":[
          {
            "title":"Rust 공식 사이트",
            "url":"https://www.rust-lang.org/",
            "description":"신뢰할 수 있는 설명",
            "extra_snippets":["추가 문맥 1","추가 문맥 2"]
          },
          {
            "title":"중복",
            "url":"https://www.rust-lang.org/",
            "description":"중복 결과"
          },
          {
            "title":"중복 2",
            "url":"https://www.rust-lang.org/",
            "description":"중복 결과"
          },
          {
            "title":"중복 3",
            "url":"https://www.rust-lang.org/",
            "description":"중복 결과"
          },
          {
            "title":"두 번째 고유 출처",
            "url":"https://doc.rust-lang.org/",
            "description":"중복 이후에도 포함되어야 함"
          },
          {
            "title":"위험",
            "url":"http://example.com/",
            "description":"HTTPS가 아님"
          }
        ]
      }
    }"#;

    #[test]
    fn parses_brave_results_and_deduplicates_https_sources() {
        let evidence = parse_search_response(FIXTURE).unwrap();

        assert!(evidence.context.contains("Rust 공식 사이트"));
        assert!(evidence.context.contains("추가 문맥 2"));
        assert_eq!(
            evidence.sources,
            vec!["https://www.rust-lang.org/", "https://doc.rust-lang.org/"]
        );
        assert!(!evidence.context.contains("HTTPS가 아님"));
    }

    #[test]
    fn rejects_empty_oversized_and_control_character_queries() {
        assert!(validate_query("").is_err());
        assert!(validate_query(&"가".repeat(MAX_QUERY_CHARS + 1)).is_err());
        assert!(validate_query(&vec!["word"; MAX_QUERY_WORDS + 1].join(" ")).is_err());
        assert!(validate_query("safe\u{0}unsafe").is_err());
        assert_eq!(validate_query(" Rust 검색 ").unwrap(), "Rust 검색");
    }

    #[test]
    fn missing_or_conflicting_api_key_is_actionable_without_values() {
        let missing = configured_api_key_from(None, None).unwrap_err().message;
        assert!(missing.contains("BRAVE_SEARCH_API_KEY"));

        let conflict = configured_api_key_from(Some("secret-a"), Some("secret-b"))
            .unwrap_err()
            .message;
        assert!(conflict.contains("서로 다릅니다"));
        assert!(!conflict.contains("secret-a"));
        assert!(!conflict.contains("secret-b"));
        assert_eq!(
            configured_api_key_from(Some("same"), Some("same")).unwrap(),
            "same"
        );
    }

    #[test]
    fn configuration_summary_reports_state_without_exposing_credentials() {
        assert_eq!(
            configuration_summary_from(Some("secret"), None),
            "사용 가능; Brave direct REST, environment-only key"
        );
        let conflict = configuration_summary_from(Some("secret-a"), Some("secret-b"));
        assert!(conflict.contains("설정 충돌"));
        assert!(!conflict.contains("secret-a"));
        assert_eq!(
            configuration_summary_from(None, None),
            "미설정; BRAVE_SEARCH_API_KEY 필요"
        );
    }

    #[test]
    fn credential_request_is_https_only_and_does_not_follow_redirects() {
        let config = brave_agent_config();

        assert!(config.https_only());
        assert_eq!(config.max_redirects(), 0);
    }

    #[test]
    fn maps_status_without_exposing_provider_response_or_key() {
        for (status, expected) in [
            (401, "인증"),
            (403, "인증"),
            (429, "한도"),
            (400, "거부"),
            (500, "일시적"),
        ] {
            let message = map_search_error(ureq::Error::StatusCode(status)).message;
            assert!(message.contains(expected), "status={status}: {message}");
            assert!(!message.contains("secret"));
        }
    }

    #[test]
    fn bounds_context_and_only_exposes_sources_inside_it() {
        let long = SearchResult {
            title: "첫 결과".to_string(),
            url: "https://example.com/first".to_string(),
            description: "가".repeat(MAX_SEARCH_CONTEXT_CHARS * 2),
            extra_snippets: Vec::new(),
        };
        let truncated = SearchResult {
            title: "잘린 결과".to_string(),
            url: "https://example.com/truncated".to_string(),
            description: "두 번째".to_string(),
            extra_snippets: Vec::new(),
        };

        let evidence = evidence_from_results(&[long, truncated]).unwrap();

        assert!(evidence.context.chars().count() <= MAX_SEARCH_CONTEXT_CHARS);
        assert_eq!(evidence.sources, vec!["https://example.com/first"]);
    }

    #[test]
    fn rejects_malformed_or_deceptive_https_sources() {
        for url in [
            "https://",
            "https://user@example.com/docs",
            "https://example.com/a path",
            "https://example.com/\nforged",
            "http://example.com/docs",
        ] {
            assert!(!is_valid_https_source_url(url), "url: {url}");
        }
        assert!(is_valid_https_source_url("https://example.com/docs?q=rust"));
    }

    #[test]
    fn live_web_search_smoke_when_explicitly_enabled_and_configured() {
        if std::env::var("RPOTATO_RUN_LIVE_WEB_SEARCH").as_deref() != Ok("1")
            || configured_api_key().is_err()
        {
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
