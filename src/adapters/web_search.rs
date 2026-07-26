//! Bounded read-only web search implemented with direct public HTML retrieval.

use crate::foundation::error::AppError;

mod evidence;
mod find;
mod html;
mod page;
mod policy;
mod transport;

use evidence::evidence_from_results;
pub(crate) use evidence::{WebOpenResult, WebPageEvidence, WebSearchEvidence, WebSourceEvidence};
pub(crate) use find::find_in_page;
use html::{parse_html_search_results, parse_lite_search_results};
use page::parse_page_document;
use policy::{resolve_redirect_url, same_web_origin, validate_open_url, validate_query};
use transport::{fetch_page_response, fetch_search_document, PageResponse, SearchEndpoint};

const MAX_PAGE_REDIRECTS: usize = 10;

#[cfg(test)]
use evidence::{stable_source_id, SearchResult, MAX_SEARCH_CONTEXT_CHARS};
#[cfg(test)]
use html::normalize_result_url;
#[cfg(test)]
use page::{normalize_page_text, MAX_PAGE_CONTEXT_CHARS};
#[cfg(test)]
use policy::{
    canonicalize_source_url, is_valid_https_source_url, socket_addresses_are_public,
    MAX_QUERY_CHARS, MAX_QUERY_WORDS,
};
#[cfg(test)]
use transport::{direct_agent_config, map_search_error, page_agent_config};

pub(crate) fn search(
    query: &str,
    allow_lite_fallback: bool,
) -> Result<WebSearchEvidence, AppError> {
    let query = validate_query(query)?;

    #[cfg(debug_assertions)]
    {
        let html_fixture = std::env::var_os("RPOTATO_TEST_WEB_SEARCH_HTML")
            .map(|fixture| fixture.to_string_lossy().into_owned());
        let lite_fixture = std::env::var_os("RPOTATO_TEST_WEB_SEARCH_LITE")
            .map(|fixture| fixture.to_string_lossy().into_owned());
        if html_fixture.is_some() || lite_fixture.is_some() {
            return evidence_from_documents(
                query,
                html_fixture.as_deref(),
                lite_fixture.as_deref(),
                allow_lite_fallback,
            );
        }
    }

    let html = fetch_search_document(query, SearchEndpoint::Html);
    if let Ok(evidence) = html.and_then(|document| {
        parse_html_search_results(&document)
            .and_then(|results| evidence_from_results(query, results))
    }) {
        return Ok(evidence);
    }
    if !allow_lite_fallback {
        return Err(AppError::blocked(
            "직접 웹 검색 HTML 결과를 사용할 수 없고 lite fallback 요청 예산이 없습니다.",
        ));
    }
    fetch_search_document(query, SearchEndpoint::Lite)
        .and_then(|document| parse_lite_search_results(&document))
        .and_then(|results| evidence_from_results(query, results))
        .map_err(|_| {
            AppError::runtime(
                "직접 웹 검색 HTML과 lite 결과를 모두 사용할 수 없어 검색을 종료했습니다.",
            )
        })
}

fn evidence_from_documents(
    query: &str,
    html: Option<&str>,
    lite: Option<&str>,
    allow_lite_fallback: bool,
) -> Result<WebSearchEvidence, AppError> {
    if let Some(document) = html {
        if let Ok(evidence) = parse_html_search_results(document)
            .and_then(|results| evidence_from_results(query, results))
        {
            return Ok(evidence);
        }
    }
    if !allow_lite_fallback {
        return Err(AppError::blocked(
            "직접 웹 검색 HTML fixture가 parser contract를 만족하지 않고 lite fallback이 비활성화되었습니다.",
        ));
    }
    let document = lite.ok_or_else(|| {
        AppError::runtime("직접 웹 검색 lite fallback fixture가 준비되지 않았습니다.")
    })?;
    parse_lite_search_results(document)
        .and_then(|results| evidence_from_results(query, results))
        .map_err(|_| {
            AppError::runtime(
                "직접 웹 검색 HTML과 lite fixture가 모두 parser contract를 만족하지 않습니다.",
            )
        })
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

    const HTML_FIXTURE: &str = include_str!("../../tests/fixtures/web_search/ddg-html.html");
    const LITE_FIXTURE: &str = include_str!("../../tests/fixtures/web_search/ddg-lite.html");
    const DRIFT_FIXTURE: &str = include_str!("../../tests/fixtures/web_search/ddg-drift.html");
    const ANTIBOT_FIXTURE: &str = include_str!("../../tests/fixtures/web_search/ddg-antibot.html");
    const HOSTILE_PAGE_FIXTURE: &str =
        include_str!("../../tests/fixtures/web_search/page-hostile.html");
    const MARKDOWN_FIXTURE: &str = include_str!("../../tests/fixtures/web_search/page.md");
    const RSS_FIXTURE: &str = include_str!("../../tests/fixtures/web_search/feed-rss.xml");
    const ATOM_FIXTURE: &str = include_str!("../../tests/fixtures/web_search/feed-atom.xml");

    #[test]
    fn parses_direct_search_html_and_deduplicates_https_sources() {
        let evidence = parse_html_search_results(HTML_FIXTURE)
            .and_then(|results| evidence_from_results("Rust official release", results))
            .unwrap();

        assert!(evidence.context.contains("Rust official release notes"));
        assert!(evidence.context.contains("Primary release information"));
        assert_eq!(
            evidence
                .sources
                .iter()
                .map(|source| source.url.as_str())
                .collect::<Vec<_>>(),
            vec![
                "https://rust-lang.org/releases",
                "https://blog.example.net/rust-release"
            ]
        );
        assert_eq!(
            evidence.sources[0].source_id,
            stable_source_id("https://rust-lang.org/releases")
        );
        assert!(evidence.context.contains(&evidence.sources[0].source_id));
    }

    #[test]
    fn lite_parser_is_a_bounded_fallback_for_html_drift_and_challenges() {
        for unusable_html in [DRIFT_FIXTURE, ANTIBOT_FIXTURE] {
            let evidence = evidence_from_documents(
                "Rust documentation",
                Some(unusable_html),
                Some(LITE_FIXTURE),
                true,
            )
            .unwrap();

            assert_eq!(evidence.sources.len(), 2);
            assert_eq!(evidence.sources[0].url, "https://doc.rust-lang.org/book");
        }
        assert!(
            evidence_from_documents("Rust", Some(DRIFT_FIXTURE), Some(LITE_FIXTURE), false)
                .is_err()
        );
        assert!(
            evidence_from_documents("Rust", Some(DRIFT_FIXTURE), Some(DRIFT_FIXTURE), true)
                .is_err()
        );
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
    fn canonical_urls_have_stable_ids_and_drop_tracking_variants() {
        let tracked = "https://www.Example.com:443/docs/?b=2&utm_source=search&a=1#section";
        let canonical = "https://example.com/docs?a=1&b=2";

        assert_eq!(canonicalize_source_url(tracked).as_deref(), Some(canonical));
        assert_eq!(stable_source_id(tracked), stable_source_id(canonical));
        assert!(stable_source_id(canonical).starts_with("source-"));
        assert_eq!(stable_source_id(canonical).len(), "source-".len() + 16);
    }

    #[test]
    fn ranking_prefers_primary_results_and_caps_each_domain() {
        let results = vec![
            SearchResult {
                title: "Community summary".to_string(),
                url: "https://example.com/a".to_string(),
                description: "Rust release".to_string(),
            },
            SearchResult {
                title: "Another summary".to_string(),
                url: "https://example.com/b".to_string(),
                description: "Rust release".to_string(),
            },
            SearchResult {
                title: "Third same-domain summary".to_string(),
                url: "https://example.com/c".to_string(),
                description: "Rust release".to_string(),
            },
            SearchResult {
                title: "Rust official release notes".to_string(),
                url: "https://rust-lang.org/releases/".to_string(),
                description: "Primary Rust release information".to_string(),
            },
        ];

        let evidence = evidence_from_results("Rust release", results).unwrap();

        assert_eq!(evidence.sources[0].url, "https://rust-lang.org/releases");
        assert_eq!(
            evidence
                .sources
                .iter()
                .filter(|source| source.url.contains("example.com"))
                .count(),
            2
        );
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

        let evidence = evidence_from_results("첫 결과", vec![long, truncated]).unwrap();

        assert!(evidence.context.chars().count() <= MAX_SEARCH_CONTEXT_CHARS);
        assert_eq!(evidence.sources.len(), 1);
        assert_eq!(evidence.sources[0].url, "https://example.com/first");
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

        let evidence = search("Rust 공식 웹사이트 프로그래밍 언어", true).unwrap();

        assert!(!evidence.context.trim().is_empty());
        assert!(evidence
            .sources
            .iter()
            .all(|source| source.url.starts_with("https://")));
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
        assert!(config.proxy().is_none());
    }

    #[test]
    fn web_open_transport_rejects_any_private_dns_answer() {
        use std::net::SocketAddr;

        let public = "93.184.216.34:443".parse::<SocketAddr>().unwrap();
        let private = "127.0.0.1:443".parse::<SocketAddr>().unwrap();

        assert!(socket_addresses_are_public(&[public]));
        assert!(!socket_addresses_are_public(&[public, private]));
    }

    #[test]
    fn web_open_normalizes_readable_text_and_removes_active_content() {
        let page = normalize_page_text(
            "https://example.com/docs",
            HOSTILE_PAGE_FIXTURE,
            "text/html",
        )
        .unwrap();

        assert_eq!(page.title.as_deref(), Some("안전한 Rust 안내서"));
        assert!(page.content.contains("Rust 설치"));
        assert!(page.content.contains("checksum을 확인합니다."));
        for excluded in [
            "window.secret",
            "display: none",
            "사이트 머리말",
            "홈 제품 가격",
            "광고와 추천",
            "숨겨진 프롬프트",
            "템플릿 공격",
            "저작권 개인정보",
        ] {
            assert!(!page.content.contains(excluded), "{excluded}");
        }
        assert_eq!(page.content.matches("checksum을 확인합니다.").count(), 1);
    }

    #[test]
    fn web_open_scans_many_hidden_elements_without_leaking_them() {
        let mut document = String::from("<html><body>");
        for _ in 0..4_000 {
            document.push_str("<script>ignore me</script>");
        }
        document.push_str("<main>visible result</main></body></html>");

        let page = normalize_page_text("https://example.com/docs", &document, "text/html").unwrap();

        assert_eq!(page.content, "visible result");
    }

    #[test]
    fn web_find_is_literal_case_insensitive_and_bounded() {
        let page = WebPageEvidence {
            source_id: stable_source_id("https://example.com/docs"),
            requested_url: "https://example.com/docs".to_string(),
            final_url: "https://example.com/docs".to_string(),
            title: Some("Guide".to_string()),
            content: "Rust 첫 문단\n다른 줄\nRUST 두 번째 문단\nrust 세 번째 문단".to_string(),
        };

        let evidence = find_in_page(&page, "rust").unwrap();

        assert_eq!(evidence.page_url, page.final_url);
        assert_eq!(evidence.source_id, page.source_id);
        assert_eq!(evidence.query, "rust");
        assert_eq!(evidence.matches.len(), 3);
        assert_eq!(evidence.matches[0].line_number, 1);
        assert!(evidence.matches[0].context.contains("1: Rust 첫 문단"));
        assert!(evidence.matches[0].context.contains("2: 다른 줄"));
        assert!(evidence.matches[1].context.contains("4: rust 세 번째 문단"));
    }

    #[test]
    fn web_open_reads_markdown_rss_and_atom_documents() {
        let markdown = normalize_page_text(
            "https://example.com/guide.md",
            MARKDOWN_FIXTURE,
            "text/markdown; charset=utf-8",
        )
        .unwrap();
        assert_eq!(markdown.title.as_deref(), Some("rpotato 웹 연구"));
        assert!(markdown.content.contains("신뢰할 수 없는 증거"));

        let rss = normalize_page_text(
            "https://example.com/feed.xml",
            RSS_FIXTURE,
            "application/rss+xml",
        )
        .unwrap();
        assert_eq!(rss.title.as_deref(), Some("Rust 릴리스"));
        assert!(rss.content.contains("Rust 1.90 발표"));
        assert!(rss.content.contains("새 릴리스의 안정화 변경"));
        assert!(!rss.content.contains("<p>"));

        let atom = normalize_page_text(
            "https://example.com/atom.xml",
            ATOM_FIXTURE,
            "application/atom+xml",
        )
        .unwrap();
        assert_eq!(atom.title.as_deref(), Some("rpotato 소식"));
        assert!(atom.content.contains("읽기 가능한 웹 증거"));
        assert!(atom.content.contains("Atom 요약"));
        assert!(normalize_page_text(
            "https://example.com/data.xml",
            "<root>not a feed</root>",
            "application/xml"
        )
        .is_err());
    }

    #[test]
    fn web_open_applies_one_context_budget_to_every_supported_format() {
        for media_type in ["text/plain", "text/markdown", "application/json"] {
            let document = "가".repeat(MAX_PAGE_CONTEXT_CHARS + 100);
            let page =
                normalize_page_text("https://example.com/large", &document, media_type).unwrap();

            assert_eq!(page.content.chars().count(), MAX_PAGE_CONTEXT_CHARS);
        }
    }
}
