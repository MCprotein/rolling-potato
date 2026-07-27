use super::*;

#[test]
fn page_find_requires_an_open_page_and_renders_context_with_source_id() {
    assert!(find_in_page(None, "Rust").is_err());
    let page = web_search::WebPageEvidence {
        source_id: "source-test".to_string(),
        requested_url: "https://example.com".to_string(),
        final_url: "https://example.com/docs".to_string(),
        title: Some("Guide".to_string()),
        content: "Rust guide\nother".to_string(),
    };
    let report = find_in_page(Some(&page), "rust").unwrap();

    for expected in [
        "일치: 1개",
        "출처: [source-test]",
        "1. 일치 줄 1",
        "1: Rust guide",
        "https://example.com/docs",
    ] {
        assert!(report.contains(expected), "{expected}");
    }
}
