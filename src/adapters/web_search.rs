//! Bounded read-only web search implemented with direct public HTML retrieval.

use crate::foundation::error::AppError;

mod evidence;
mod html;
mod policy;
mod transport;

pub(crate) use evidence::WebSearchEvidence;
use html::parse_search_document;
use policy::validate_query;
use transport::fetch_search_document;

#[cfg(test)]
use evidence::{evidence_from_results, SearchResult, MAX_SEARCH_CONTEXT_CHARS};
#[cfg(test)]
use html::normalize_result_url;
#[cfg(test)]
use policy::{is_valid_https_source_url, MAX_QUERY_CHARS, MAX_QUERY_WORDS};
#[cfg(test)]
use transport::{direct_agent_config, map_search_error};

pub(crate) fn search(query: &str) -> Result<WebSearchEvidence, AppError> {
    let query = validate_query(query)?;

    #[cfg(debug_assertions)]
    if let Some(fixture) = std::env::var_os("RPOTATO_TEST_WEB_SEARCH_HTML") {
        return parse_search_document(&fixture.to_string_lossy());
    }

    let document = fetch_search_document(query)?;
    parse_search_document(&document)
}

pub(crate) fn configuration_summary() -> String {
    "사용 가능; API key 없는 직접 웹 검색".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
      <div class="result results_links web-result">
        <h2 class="result__title">
          <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&amp;rut=ignored">
            Rust <b>공식</b> 사이트
          </a>
        </h2>
        <a class="result__snippet">신뢰할 수 있는 설명 &amp; 추가 문맥</a>
      </div>
      <div class="result results_links web-result">
        <h2 class="result__title">
          <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F&amp;rut=duplicate">
            중복
          </a>
        </h2>
        <a class="result__snippet">중복 결과</a>
      </div>
      <div class="result results_links web-result">
        <h2 class="result__title">
          <a class="result__a" href="https://doc.rust-lang.org/">
            두 번째 고유 출처
          </a>
        </h2>
        <a class="result__snippet">중복 이후에도 포함되어야 함</a>
      </div>
      <div class="result results_links web-result">
        <h2 class="result__title">
          <a class="result__a" href="//duckduckgo.com/l/?uddg=http%3A%2F%2Fexample.com%2F">
            위험
          </a>
        </h2>
        <a class="result__snippet">HTTPS가 아님</a>
      </div>
    "#;

    #[test]
    fn parses_direct_search_html_and_deduplicates_https_sources() {
        let evidence = parse_search_document(FIXTURE).unwrap();

        assert!(evidence.context.contains("Rust 공식 사이트"));
        assert!(evidence.context.contains("설명 & 추가 문맥"));
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
    fn direct_search_is_available_without_api_credentials() {
        assert_eq!(
            configuration_summary(),
            "사용 가능; API key 없는 직접 웹 검색"
        );
    }

    #[test]
    fn unwraps_only_valid_https_result_targets() {
        assert_eq!(
            normalize_result_url(
                "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fdocs%3Fq%3Drust&amp;rut=x"
            )
            .as_deref(),
            Some("https://example.com/docs?q=rust")
        );
        assert_eq!(
            normalize_result_url("https://example.com/direct").as_deref(),
            Some("https://example.com/direct")
        );
        assert!(
            normalize_result_url("//duckduckgo.com/l/?uddg=http%3A%2F%2Fexample.com%2F").is_none()
        );
        assert!(normalize_result_url("//duckduckgo.com/l/?rut=missing").is_none());
    }

    #[test]
    fn direct_request_is_https_only_and_does_not_follow_redirects() {
        let config = direct_agent_config();

        assert!(config.https_only());
        assert_eq!(config.max_redirects(), 0);
    }

    #[test]
    fn maps_status_without_exposing_provider_response() {
        for (status, expected) in [(429, "요청"), (400, "거부"), (500, "일시적")] {
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
        };
        let truncated = SearchResult {
            title: "잘린 결과".to_string(),
            url: "https://example.com/truncated".to_string(),
            description: "두 번째".to_string(),
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
