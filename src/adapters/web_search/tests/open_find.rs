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
