//! Bounded read-only web search implemented with direct public HTML retrieval.

use crate::foundation::error::AppError;

mod evidence;
mod find;
mod html;
mod page;
mod policy;
mod transport;

pub(crate) use evidence::{WebOpenResult, WebPageEvidence, WebSearchEvidence};
pub(crate) use find::find_in_page;
use html::parse_search_document;
use page::parse_page_document;
use policy::{
    resolve_redirect_url, same_web_origin, validate_open_url, validate_public_resolution,
    validate_query,
};
use transport::{fetch_page_response, fetch_search_document, PageResponse};

const MAX_PAGE_REDIRECTS: usize = 10;

#[cfg(test)]
use evidence::{evidence_from_results, SearchResult, MAX_SEARCH_CONTEXT_CHARS};
#[cfg(test)]
use html::normalize_result_url;
#[cfg(test)]
use page::normalize_page_text;
#[cfg(test)]
use policy::{is_valid_https_source_url, MAX_QUERY_CHARS, MAX_QUERY_WORDS};
#[cfg(test)]
use transport::{direct_agent_config, map_search_error, page_agent_config};

pub(crate) fn search(query: &str) -> Result<WebSearchEvidence, AppError> {
    let query = validate_query(query)?;

    #[cfg(debug_assertions)]
    if let Some(fixture) = std::env::var_os("RPOTATO_TEST_WEB_SEARCH_HTML") {
        return parse_search_document(&fixture.to_string_lossy());
    }

    let document = fetch_search_document(query)?;
    parse_search_document(&document)
}

pub(crate) fn open(url: &str) -> Result<WebOpenResult, AppError> {
    let requested_url = validate_open_url(url)?;

    #[cfg(debug_assertions)]
    if let Some(fixture) = std::env::var_os("RPOTATO_TEST_WEB_OPEN_HTML") {
        return parse_page_document(
            &requested_url,
            &requested_url,
            &fixture.to_string_lossy(),
            "text/html",
        )
        .map(WebOpenResult::Opened);
    }

    let mut current_url = requested_url.clone();
    for redirect_count in 0..=MAX_PAGE_REDIRECTS {
        validate_public_resolution(&current_url)?;
        match fetch_page_response(&current_url)? {
            PageResponse::Document { content_type, body } => {
                return parse_page_document(&requested_url, &current_url, &body, &content_type)
                    .map(WebOpenResult::Opened);
            }
            PageResponse::Redirect { location } => {
                let target_url = resolve_redirect_url(&current_url, &location)?;
                if !same_web_origin(&current_url, &target_url) {
                    return Ok(WebOpenResult::Redirect {
                        from_url: current_url,
                        target_url,
                    });
                }
                if redirect_count == MAX_PAGE_REDIRECTS {
                    return Err(AppError::blocked(
                        "WebOpen 동일 host redirect가 10회를 초과했습니다.",
                    ));
                }
                current_url = target_url;
            }
        }
    }
    unreachable!("redirect loop returns at its bounded terminal state")
}

pub(crate) fn configuration_summary() -> String {
    "사용 가능; API key 없는 WebSearch·WebOpen·WebFind".to_string()
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
            "사용 가능; API key 없는 WebSearch·WebOpen·WebFind"
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

    #[test]
    fn live_web_open_smoke_when_explicitly_enabled() {
        if std::env::var("RPOTATO_RUN_LIVE_WEB_OPEN").as_deref() != Ok("1") {
            return;
        }

        let result = open("https://example.com/").unwrap();
        let WebOpenResult::Opened(page) = result else {
            panic!("example.com must not cross-host redirect");
        };

        assert_eq!(page.final_url, "https://example.com/");
        assert!(!page.content.trim().is_empty());
    }

    #[test]
    fn web_open_upgrades_http_and_rejects_private_or_credentialed_targets() {
        assert_eq!(
            validate_open_url("http://example.com/docs").unwrap(),
            "https://example.com/docs"
        );
        for url in [
            "https://user:secret@example.com/",
            "https://localhost/",
            "https://127.0.0.1/",
            "https://10.0.0.1/",
            "https://[::1]/",
            "file:///tmp/secret",
        ] {
            assert!(validate_open_url(url).is_err(), "url: {url}");
        }
    }

    #[test]
    fn web_open_only_auto_follows_same_host_redirects() {
        let current = "https://docs.example.com/guide/start";
        let same = resolve_redirect_url(current, "/guide/next").unwrap();
        let www = resolve_redirect_url(current, "https://www.docs.example.com/guide").unwrap();
        let cross = resolve_redirect_url(current, "https://accounts.example.net/login").unwrap();

        assert!(same_web_origin(current, &same));
        assert!(same_web_origin(current, &www));
        assert!(!same_web_origin(current, &cross));
        assert_eq!(same, "https://docs.example.com/guide/next");
    }

    #[test]
    fn web_open_transport_never_auto_follows_redirects() {
        let config = page_agent_config();

        assert!(config.https_only());
        assert_eq!(config.max_redirects(), 0);
    }

    #[test]
    fn web_open_normalizes_readable_text_and_removes_active_content() {
        let document = r#"
            <html><head><title>Rust &amp; 안전</title>
            <style>.secret { display:none }</style>
            <script>alert("ignore")</script></head>
            <body><nav>메뉴</nav><main><h1>시작</h1><p>Rust 문서입니다.</p></main></body></html>
        "#;

        let page = normalize_page_text("https://example.com/docs", document, "text/html").unwrap();

        assert_eq!(page.title.as_deref(), Some("Rust & 안전"));
        assert!(page.content.contains("시작"));
        assert!(page.content.contains("Rust 문서입니다."));
        assert!(!page.content.contains("alert"));
        assert!(!page.content.contains("display:none"));
    }

    #[test]
    fn web_find_is_literal_case_insensitive_and_bounded() {
        let page = WebPageEvidence {
            requested_url: "https://example.com/docs".to_string(),
            final_url: "https://example.com/docs".to_string(),
            title: Some("Guide".to_string()),
            content: "Rust 첫 문단\n다른 줄\nRUST 두 번째 문단\nrust 세 번째 문단".to_string(),
        };

        let evidence = find_in_page(&page, "rust").unwrap();

        assert_eq!(evidence.page_url, page.final_url);
        assert_eq!(evidence.query, "rust");
        assert_eq!(evidence.matches.len(), 3);
        assert!(evidence.matches[0].contains("Rust 첫 문단"));
    }
}
