use super::*;

fn page(index: usize) -> WebPageEvidence {
    WebPageEvidence {
        source_id: format!("source-{index:016x}"),
        requested_url: format!("https://example.com/{index}"),
        final_url: format!("https://example.com/{index}"),
        title: Some(format!("Page {index}")),
        content: format!("content {index}"),
    }
}

#[test]
fn page_session_evicts_the_oldest_entry_and_keeps_a_current_page() {
    let mut session = WebPageSession::default();
    for index in 0..=MAX_OPEN_PAGES {
        session.record(page(index));
    }

    assert_eq!(session.len(), MAX_OPEN_PAGES);
    assert_eq!(
        session.current().unwrap().source_id,
        page(MAX_OPEN_PAGES).source_id
    );
    assert!(!session.select(&page(0).source_id));
    assert!(session.select(&page(1).source_id));
    assert_eq!(session.current().unwrap().source_id, page(1).source_id);
}

#[test]
fn reopening_a_source_refreshes_it_without_creating_a_duplicate() {
    let mut session = WebPageSession::default();
    session.record(page(1));
    let mut refreshed = page(1);
    refreshed.content = "refreshed".to_string();
    session.record(refreshed);

    assert_eq!(session.len(), 1);
    assert_eq!(session.current().unwrap().content, "refreshed");
    assert_eq!(session.pages.len(), 1);
}

#[test]
fn discovered_results_join_open_pages_without_duplicate_source_ids() {
    let mut session = WebPageSession::default();
    session.record_discovered_sources(vec![
        WebSourceEvidence {
            source_id: page(1).source_id,
            title: "Search result one".to_string(),
            url: page(1).final_url,
        },
        WebSourceEvidence {
            source_id: page(2).source_id,
            title: "Search result two".to_string(),
            url: page(2).final_url,
        },
    ]);
    session.record(page(1));

    let sources = session.sources();

    assert_eq!(sources.len(), 2);
    assert!(sources[0].opened);
    assert!(sources[0].current);
    assert!(!sources[1].opened);
    assert_eq!(sources[1].source_id, page(2).source_id);
}
