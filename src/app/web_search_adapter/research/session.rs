use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::types::{
    bounded_input, valid_step, FailedInputAction, WebResearchAdmission, WebResearchBudget,
    WebResearchLimit, WebResearchStep, WebResearchTerminal,
};

const MAX_FAILED_INPUT_ATTEMPTS: u8 = 2;

#[derive(Debug)]
pub(crate) struct WebResearchSession {
    budget: WebResearchBudget,
    steps: u8,
    searches: u8,
    opens: u8,
    network_requests: u8,
    evidence_bytes: usize,
    seen_queries: BTreeSet<String>,
    finds_by_document: BTreeMap<String, u8>,
    opened_documents: BTreeSet<String>,
    failed_inputs: BTreeMap<WebResearchStep, u8>,
    terminal: Option<WebResearchTerminal>,
}

impl Default for WebResearchSession {
    fn default() -> Self {
        Self::new(WebResearchBudget::default())
    }
}

impl WebResearchSession {
    pub(crate) fn new(budget: WebResearchBudget) -> Self {
        Self {
            budget,
            steps: 0,
            searches: 0,
            opens: 0,
            network_requests: 0,
            evidence_bytes: 0,
            seen_queries: BTreeSet::new(),
            finds_by_document: BTreeMap::new(),
            opened_documents: BTreeSet::new(),
            failed_inputs: BTreeMap::new(),
            terminal: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_evidence_limit(max_evidence_bytes: usize) -> Self {
        Self::new(WebResearchBudget {
            max_evidence_bytes,
            ..WebResearchBudget::default()
        })
    }

    pub(crate) fn admit(
        &mut self,
        step: WebResearchStep,
        current_document: Option<&str>,
        elapsed: Duration,
    ) -> WebResearchAdmission {
        if let Some(terminal) = self.terminal {
            return WebResearchAdmission::Stop(terminal);
        }
        if elapsed >= self.budget.max_elapsed {
            return self.stop(WebResearchTerminal::BudgetReached(
                WebResearchLimit::Elapsed,
            ));
        }
        if !valid_step(&step) {
            return self.stop(WebResearchTerminal::InvalidStep);
        }
        if self.steps >= self.budget.max_steps {
            return self.stop(WebResearchTerminal::BudgetReached(WebResearchLimit::Steps));
        }
        if step.needs_network() && self.network_requests >= self.budget.max_network_requests {
            return self.stop(WebResearchTerminal::BudgetReached(
                WebResearchLimit::NetworkRequests,
            ));
        }

        match &step {
            WebResearchStep::Search { query } => {
                if self.searches >= self.budget.max_searches {
                    return self.stop(WebResearchTerminal::BudgetReached(
                        WebResearchLimit::Searches,
                    ));
                }
                let is_revision =
                    !self.seen_queries.is_empty() && !self.seen_queries.contains(query.trim());
                let revisions = self.seen_queries.len().saturating_sub(1);
                if is_revision && revisions >= usize::from(self.budget.max_query_revisions) {
                    return self.stop(WebResearchTerminal::BudgetReached(
                        WebResearchLimit::QueryRevisions,
                    ));
                }
                self.searches += 1;
                self.seen_queries.insert(query.trim().to_string());
            }
            WebResearchStep::Open { .. } => {
                if self.opens >= self.budget.max_opens {
                    return self.stop(WebResearchTerminal::BudgetReached(WebResearchLimit::Opens));
                }
                self.opens += 1;
            }
            WebResearchStep::Find { .. } => {
                let Some(document) = current_document
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    return self.stop(WebResearchTerminal::InvalidStep);
                };
                let finds = self
                    .finds_by_document
                    .get(document)
                    .copied()
                    .unwrap_or_default();
                if finds >= self.budget.max_finds_per_document {
                    return self.stop(WebResearchTerminal::BudgetReached(
                        WebResearchLimit::FindsPerDocument,
                    ));
                }
                self.finds_by_document
                    .insert(document.to_string(), finds + 1);
            }
        }

        self.steps += 1;
        if step.needs_network() {
            self.network_requests += 1;
        }
        WebResearchAdmission::Execute(step)
    }

    pub(crate) fn take_evidence(&mut self, evidence: &str) -> String {
        if self.terminal.is_some() {
            return String::new();
        }
        let remaining = self
            .budget
            .max_evidence_bytes
            .saturating_sub(self.evidence_bytes);
        if remaining == 0 && !evidence.is_empty() {
            return String::new();
        }
        let end = evidence
            .char_indices()
            .take_while(|(index, character)| index + character.len_utf8() <= remaining)
            .map(|(index, character)| index + character.len_utf8())
            .last()
            .unwrap_or(0);
        if end == 0 && !evidence.is_empty() {
            self.evidence_bytes = self.budget.max_evidence_bytes;
            return String::new();
        }
        let bounded = evidence[..end].to_string();
        self.evidence_bytes = self.evidence_bytes.saturating_add(bounded.len());
        bounded
    }

    pub(crate) fn has_evidence_capacity(&self) -> bool {
        self.terminal.is_none() && self.evidence_bytes < self.budget.max_evidence_bytes
    }

    pub(crate) fn reserve_optional_network_request(&mut self, elapsed: Duration) -> bool {
        if self.terminal.is_some()
            || elapsed >= self.budget.max_elapsed
            || self.network_requests >= self.budget.max_network_requests
        {
            return false;
        }
        self.network_requests += 1;
        true
    }

    pub(crate) fn record_opened_document(&mut self, url: &str) {
        let url = url.trim();
        if !url.is_empty() {
            self.opened_documents.insert(url.to_string());
        }
    }

    pub(crate) fn record_failed_input(&mut self, step: &WebResearchStep) -> FailedInputAction {
        let attempts = self.failed_inputs.entry(step.clone()).or_default();
        *attempts = attempts.saturating_add(1);
        if *attempts < MAX_FAILED_INPUT_ATTEMPTS {
            FailedInputAction::Retry
        } else {
            FailedInputAction::UseFallback
        }
    }

    pub(crate) fn deterministic_fallback(
        &mut self,
        original_query: &str,
        candidate_urls: &[String],
        elapsed: Duration,
    ) -> WebResearchAdmission {
        if let Some(terminal) = self.terminal {
            return WebResearchAdmission::Stop(terminal);
        }
        if self.searches == 0 {
            return self.admit(
                WebResearchStep::Search {
                    query: bounded_input(original_query),
                },
                None,
                elapsed,
            );
        }
        if let Some(url) = candidate_urls
            .iter()
            .map(|url| url.trim())
            .find(|url| url.starts_with("https://") && !self.opened_documents.contains(*url))
        {
            return self.admit(
                WebResearchStep::Open {
                    url: url.to_string(),
                },
                None,
                elapsed,
            );
        }
        self.stop(WebResearchTerminal::NoUsableEvidence)
    }

    pub(crate) fn complete(&mut self) -> WebResearchTerminal {
        self.terminal = Some(WebResearchTerminal::Complete);
        WebResearchTerminal::Complete
    }

    fn stop(&mut self, terminal: WebResearchTerminal) -> WebResearchAdmission {
        self.terminal = Some(terminal);
        WebResearchAdmission::Stop(terminal)
    }
}
