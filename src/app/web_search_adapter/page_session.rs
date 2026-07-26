use std::collections::VecDeque;

use crate::adapters::web_search::{WebPageEvidence, WebSourceEvidence};

const MAX_OPEN_PAGES: usize = 8;
const MAX_DISCOVERED_SOURCES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebSessionSource {
    pub(crate) source_id: String,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) opened: bool,
    pub(crate) current: bool,
}

#[derive(Debug, Default)]
pub(crate) struct WebPageSession {
    pages: VecDeque<WebPageEvidence>,
    discovered_sources: VecDeque<WebSourceEvidence>,
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

    pub(crate) fn record_discovered_sources(&mut self, sources: Vec<WebSourceEvidence>) {
        self.discovered_sources = sources.into_iter().take(MAX_DISCOVERED_SOURCES).collect();
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
        self.discovered_sources.clear();
        self.current_source_id = None;
    }

    pub(crate) fn source(&self, source_id: &str) -> Option<WebSessionSource> {
        self.sources()
            .into_iter()
            .find(|source| source.source_id == source_id)
    }

    pub(crate) fn sources(&self) -> Vec<WebSessionSource> {
        let mut sources = self
            .pages
            .iter()
            .rev()
            .map(|page| WebSessionSource {
                source_id: page.source_id.clone(),
                title: page
                    .title
                    .clone()
                    .unwrap_or_else(|| "제목 없음".to_string()),
                url: page.final_url.clone(),
                opened: true,
                current: self.current_source_id.as_deref() == Some(page.source_id.as_str()),
            })
            .collect::<Vec<_>>();
        sources.extend(
            self.discovered_sources
                .iter()
                .filter(|source| {
                    !self
                        .pages
                        .iter()
                        .any(|page| page.source_id == source.source_id)
                })
                .map(|source| WebSessionSource {
                    source_id: source.source_id.clone(),
                    title: source.title.clone(),
                    url: source.url.clone(),
                    opened: false,
                    current: false,
                }),
        );
        sources
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.pages.len()
    }
}

#[cfg(test)]
mod tests;
