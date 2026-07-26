use std::collections::VecDeque;

use crate::adapters::web_search::WebPageEvidence;

const MAX_OPEN_PAGES: usize = 8;

#[derive(Debug, Default)]
pub(crate) struct WebPageSession {
    pages: VecDeque<WebPageEvidence>,
    current_source_id: Option<String>,
}

impl WebPageSession {
    pub(crate) fn current(&self) -> Option<&WebPageEvidence> {
        let source_id = self.current_source_id.as_deref()?;
        self.pages.iter().find(|page| page.source_id == source_id)
    }

    pub(crate) fn current_url(&self) -> Option<&str> {
        self.current().map(|page| page.final_url.as_str())
    }

    pub(crate) fn record(&mut self, page: WebPageEvidence) {
        let source_id = page.source_id.clone();
        if let Some(index) = self
            .pages
            .iter()
            .position(|existing| existing.source_id == source_id)
        {
            self.pages.remove(index);
        }
        self.pages.push_back(page);
        while self.pages.len() > MAX_OPEN_PAGES {
            self.pages.pop_front();
        }
        self.select(&source_id);
    }

    pub(crate) fn select(&mut self, source_id: &str) -> bool {
        if self.pages.iter().any(|page| page.source_id == source_id) {
            self.current_source_id = Some(source_id.to_string());
            true
        } else {
            false
        }
    }

    pub(crate) fn clear(&mut self) {
        self.pages.clear();
        self.current_source_id = None;
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.pages.len()
    }
}

#[cfg(test)]
mod tests {
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
}
