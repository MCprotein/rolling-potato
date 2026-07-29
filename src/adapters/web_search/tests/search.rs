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
        evidence_from_documents("Rust", Some(DRIFT_FIXTURE), Some(LITE_FIXTURE), false).is_err()
    );
    assert!(
        evidence_from_documents("Rust", Some(DRIFT_FIXTURE), Some(DRIFT_FIXTURE), true).is_err()
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
    assert!(normalize_result_url("//duckduckgo.com/l/?uddg=http%3A%2F%2Fexample.com%2F").is_none());
    assert!(normalize_result_url("//duckduckgo.com/l/?rut=missing").is_none());
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
fn ranking_prefers_authoritative_results_over_matching_user_generated_posts() {
    let results = vec![
        SearchResult {
            title: "2026 월드컵 우승국 스페인 공식 결과".to_string(),
            url: "https://blog.naver.com/example/prediction".to_string(),
            description: "개인 블로그의 대회 전망과 예상 우승국".to_string(),
        },
        SearchResult {
            title: "2026 FIFA World Cup winner and results".to_string(),
            url: "https://fifawatch.com/world-cup-2026-winner".to_string(),
            description: "World Cup result summary".to_string(),
        },
        SearchResult {
            title: "2026 FIFA World Cup results and standings".to_string(),
            url: "https://worldcupwiki.com/2026/results".to_string(),
            description: "Tournament results and standings".to_string(),
        },
        SearchResult {
            title: "Official 2026 FIFA World Cup result".to_string(),
            url: "https://official.com/world-cup-result".to_string(),
            description: "Unofficial result aggregator".to_string(),
        },
        SearchResult {
            title: "FIFA World Cup 2026 results".to_string(),
            url: "https://www.fifa.com/tournaments/mens/worldcup/canadamexicousa2026/results"
                .to_string(),
            description: "Official tournament results and match centre".to_string(),
        },
    ];

    let evidence =
        evidence_from_results("2026 FIFA 월드컵 우승 공식 official result", results).unwrap();

    assert_eq!(
        evidence.sources[0].url,
        "https://fifa.com/tournaments/mens/worldcup/canadamexicousa2026/results"
    );
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
