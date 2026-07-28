use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::foundation::error::AppError;

const MAX_TOOL_INPUT_CHARS: usize = 512;
const MAX_FAILED_INPUT_ATTEMPTS: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum WebResearchStep {
    Search { query: String },
    Open { url: String },
    Find { query: String },
}

impl WebResearchStep {
    pub(crate) fn input(&self) -> &str {
        match self {
            Self::Search { query } | Self::Find { query } => query,
            Self::Open { url } => url,
        }
    }

    fn needs_network(&self) -> bool {
        matches!(self, Self::Search { .. } | Self::Open { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WebResearchBudget {
    max_steps: u8,
    max_searches: u8,
    max_opens: u8,
    max_query_revisions: u8,
    max_finds_per_document: u8,
    max_network_requests: u8,
    max_evidence_bytes: usize,
    max_elapsed: Duration,
    final_answer_tokens: u32,
}

impl Default for WebResearchBudget {
    fn default() -> Self {
        Self {
            max_steps: 6,
            max_searches: 2,
            max_opens: 3,
            max_query_revisions: 1,
            max_finds_per_document: 2,
            max_network_requests: 6,
            max_evidence_bytes: 8 * 1024,
            max_elapsed: Duration::from_secs(45),
            final_answer_tokens: 768,
        }
    }
}

impl WebResearchBudget {
    pub(crate) fn final_answer_tokens(self) -> u32 {
        self.final_answer_tokens
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebResearchLimit {
    Steps,
    Searches,
    Opens,
    QueryRevisions,
    FindsPerDocument,
    NetworkRequests,
    Elapsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebResearchTerminal {
    Complete,
    NoUsableEvidence,
    InvalidStep,
    BudgetReached(WebResearchLimit),
}

impl WebResearchTerminal {
    pub(crate) fn into_error(self) -> AppError {
        let reason = match self {
            Self::Complete => "웹 리서치 요청이 이미 완료되었습니다.",
            Self::NoUsableEvidence => "검증 가능한 웹 자료를 찾지 못했습니다.",
            Self::InvalidStep => "웹 리서치 단계가 허용된 형식이나 순서를 벗어났습니다.",
            Self::BudgetReached(limit) => match limit {
                WebResearchLimit::Steps => "웹 도구 단계 상한에 도달했습니다.",
                WebResearchLimit::Searches => "검색 횟수 상한에 도달했습니다.",
                WebResearchLimit::Opens => "문서 열기 횟수 상한에 도달했습니다.",
                WebResearchLimit::QueryRevisions => "검색어 수정 횟수 상한에 도달했습니다.",
                WebResearchLimit::FindsPerDocument => {
                    "현재 문서의 내부 찾기 횟수 상한에 도달했습니다."
                }
                WebResearchLimit::NetworkRequests => "외부 네트워크 요청 상한에 도달했습니다.",
                WebResearchLimit::Elapsed => "웹 리서치 시간 상한에 도달했습니다.",
            },
        };
        AppError::blocked(reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WebResearchAdmission {
    Execute(WebResearchStep),
    Stop(WebResearchTerminal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailedInputAction {
    Retry,
    UseFallback,
}

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

#[cfg(test)]
pub(crate) fn deterministic_freshness_fallback(request: &str) -> Option<WebResearchStep> {
    deterministic_freshness_fallback_for_context(request, &[])
}

pub(crate) fn deterministic_freshness_fallback_for_context(
    request: &str,
    prior_user_requests: &[&str],
) -> Option<WebResearchStep> {
    let request = request.trim();
    if request.is_empty() || super::routing::web_disabled(request) || !needs_fresh_web(request) {
        return None;
    }
    let query = super::routing::contextualize_search_input(request, request, prior_user_requests)?;
    let mut research = WebResearchSession::default();
    match research.deterministic_fallback(&query, &[], Duration::ZERO) {
        WebResearchAdmission::Execute(step) => Some(step),
        WebResearchAdmission::Stop(_) => None,
    }
}

fn valid_step(step: &WebResearchStep) -> bool {
    let input = step.input().trim();
    !input.is_empty()
        && !input.contains(['\r', '\n'])
        && input.chars().count() <= MAX_TOOL_INPUT_CHARS
        && match step {
            WebResearchStep::Open { url } => url.starts_with("https://"),
            WebResearchStep::Search { .. } | WebResearchStep::Find { .. } => true,
        }
}

fn bounded_input(input: &str) -> String {
    input.trim().chars().take(MAX_TOOL_INPUT_CHARS).collect()
}

fn needs_fresh_web(request: &str) -> bool {
    let lower = request.to_ascii_lowercase();
    [
        "검색해",
        "검색해서",
        "검색하여",
        "찾아줘",
        "찾아봐",
        "찾아보",
        "웹에서",
        "인터넷에서",
        "최신",
        "최근 뉴스",
        "오늘 뉴스",
        "실시간",
    ]
    .iter()
    .any(|signal| request.contains(signal))
        || [
            "search for",
            "look up",
            "browse for",
            "on the web",
            "latest",
            "breaking news",
            "real-time",
        ]
        .iter()
        .any(|signal| lower.contains(signal))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search(query: &str) -> WebResearchStep {
        WebResearchStep::Search {
            query: query.to_string(),
        }
    }

    fn open(url: &str) -> WebResearchStep {
        WebResearchStep::Open {
            url: url.to_string(),
        }
    }

    fn find(query: &str) -> WebResearchStep {
        WebResearchStep::Find {
            query: query.to_string(),
        }
    }

    #[test]
    fn default_budget_matches_the_v050_contract() {
        let budget = WebResearchBudget::default();

        assert_eq!(budget.max_steps, 6);
        assert_eq!(budget.max_searches, 2);
        assert_eq!(budget.max_opens, 3);
        assert_eq!(budget.max_query_revisions, 1);
        assert_eq!(budget.max_finds_per_document, 2);
        assert_eq!(budget.max_network_requests, 6);
        assert_eq!(budget.max_evidence_bytes, 8 * 1024);
        assert_eq!(budget.max_elapsed, Duration::from_secs(45));
        assert_eq!(budget.final_answer_tokens(), 768);
    }

    #[test]
    fn routing_budget_stops_at_search_revision_and_document_find_limits() {
        let mut research = WebResearchSession::default();
        assert_eq!(
            research.admit(search("rust release"), None, Duration::ZERO),
            WebResearchAdmission::Execute(search("rust release"))
        );
        assert_eq!(
            research.admit(search("rust stable release"), None, Duration::ZERO),
            WebResearchAdmission::Execute(search("rust stable release"))
        );
        assert_eq!(
            research.admit(search("rust current"), None, Duration::ZERO),
            WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
                WebResearchLimit::Searches
            ))
        );

        let mut research = WebResearchSession::default();
        assert!(matches!(
            research.admit(
                find("ownership"),
                Some("https://example.com/a"),
                Duration::ZERO
            ),
            WebResearchAdmission::Execute(_)
        ));
        assert!(matches!(
            research.admit(
                find("borrowing"),
                Some("https://example.com/a"),
                Duration::ZERO
            ),
            WebResearchAdmission::Execute(_)
        ));
        assert_eq!(
            research.admit(
                find("lifetimes"),
                Some("https://example.com/a"),
                Duration::ZERO
            ),
            WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
                WebResearchLimit::FindsPerDocument
            ))
        );
    }

    #[test]
    fn every_step_kind_has_an_independent_budget() {
        let revision_budget = WebResearchBudget {
            max_searches: 3,
            ..WebResearchBudget::default()
        };
        let mut revisions = WebResearchSession::new(revision_budget);
        assert!(matches!(
            revisions.admit(search("one"), None, Duration::ZERO),
            WebResearchAdmission::Execute(_)
        ));
        assert!(matches!(
            revisions.admit(search("two"), None, Duration::ZERO),
            WebResearchAdmission::Execute(_)
        ));
        assert_eq!(
            revisions.admit(search("three"), None, Duration::ZERO),
            WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
                WebResearchLimit::QueryRevisions
            ))
        );

        let step_budget = WebResearchBudget {
            max_searches: 6,
            max_query_revisions: 6,
            max_opens: 6,
            max_network_requests: 12,
            ..WebResearchBudget::default()
        };
        let mut steps = WebResearchSession::new(step_budget);
        for query in ["one", "two", "three", "four", "five", "six"] {
            assert!(matches!(
                steps.admit(search(query), None, Duration::ZERO),
                WebResearchAdmission::Execute(_)
            ));
        }
        assert_eq!(
            steps.admit(open("https://example.com/seven"), None, Duration::ZERO),
            WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(WebResearchLimit::Steps))
        );

        let mut opens = WebResearchSession::default();
        for path in ["one", "two", "three"] {
            assert!(matches!(
                opens.admit(
                    open(&format!("https://example.com/{path}")),
                    None,
                    Duration::ZERO
                ),
                WebResearchAdmission::Execute(_)
            ));
        }
        assert_eq!(
            opens.admit(open("https://example.com/four"), None, Duration::ZERO),
            WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(WebResearchLimit::Opens))
        );
    }

    #[test]
    fn elapsed_limit_is_sticky_but_evidence_exhaustion_is_a_soft_boundary() {
        let mut elapsed = WebResearchSession::default();
        assert_eq!(
            elapsed.admit(search("rust"), None, Duration::from_secs(45)),
            WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
                WebResearchLimit::Elapsed
            ))
        );
        assert_eq!(
            elapsed.admit(search("retry"), None, Duration::ZERO),
            WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
                WebResearchLimit::Elapsed
            ))
        );

        let mut evidence = WebResearchSession::default();
        let exact = evidence.take_evidence(&"a".repeat(8 * 1024));
        assert_eq!(exact.len(), 8 * 1024);
        assert_eq!(evidence.take_evidence("b"), "");
        assert!(!evidence.has_evidence_capacity());
        assert!(matches!(
            evidence.admit(open("https://example.com"), None, Duration::ZERO),
            WebResearchAdmission::Execute(_)
        ));

        let mut multibyte = WebResearchSession::default();
        let bounded = multibyte.take_evidence(&"가".repeat(4_000));
        assert!(bounded.len() <= 8 * 1024);
        assert!(bounded.is_char_boundary(bounded.len()));
        assert!(multibyte.take_evidence("가").is_empty());
        assert!(
            !multibyte.has_evidence_capacity(),
            "UTF-8 code point보다 작은 잔여 byte는 소진된 예산으로 처리해야 합니다."
        );
    }

    #[test]
    fn failed_input_retries_once_then_uses_a_deterministic_fallback() {
        let mut research = WebResearchSession::default();
        let failed = search("latest Rust");

        assert_eq!(
            research.record_failed_input(&failed),
            FailedInputAction::Retry
        );
        assert_eq!(
            research.record_failed_input(&failed),
            FailedInputAction::UseFallback
        );
        assert_eq!(
            research.deterministic_fallback("latest Rust", &[], Duration::ZERO),
            WebResearchAdmission::Execute(failed)
        );

        research.record_opened_document("https://example.com/already-open");
        let candidates = vec![
            "http://insecure.example".to_string(),
            "https://example.com/already-open".to_string(),
            "https://example.com/next".to_string(),
        ];
        assert_eq!(
            research.deterministic_fallback("latest Rust", &candidates, Duration::ZERO),
            WebResearchAdmission::Execute(open("https://example.com/next"))
        );
        research.record_opened_document("https://example.com/next");
        assert_eq!(
            research.deterministic_fallback("latest Rust", &candidates, Duration::ZERO),
            WebResearchAdmission::Stop(WebResearchTerminal::NoUsableEvidence)
        );
    }

    #[test]
    fn freshness_fallback_is_bounded_and_respects_web_opt_out() {
        assert_eq!(
            deterministic_freshness_fallback("최신 Rust 릴리스를 찾아줘"),
            Some(search("최신 Rust 릴리스를"))
        );
        assert!(deterministic_freshness_fallback(
            "인터넷 검색하지 말고 최신 Rust 릴리스를 설명해줘"
        )
        .is_none());
        assert!(deterministic_freshness_fallback("현재 파일의 함수를 설명해줘").is_none());
        let long = format!("최신 {}", "가".repeat(600));
        let Some(WebResearchStep::Search { query }) = deterministic_freshness_fallback(&long)
        else {
            panic!("freshness query should route to search");
        };
        assert_eq!(query.chars().count(), MAX_TOOL_INPUT_CHARS);
    }

    #[test]
    fn freshness_fallback_resolves_meta_search_from_recent_user_topic() {
        let Some(WebResearchStep::Search { query }) = deterministic_freshness_fallback_for_context(
            "검색해봐 끝낫어",
            &["월드컵 우승국가가 어디야", "2026년은?"],
        ) else {
            panic!("contextual freshness request did not route to search");
        };

        assert!(query.contains("월드컵"));
        assert!(query.contains("2026"));
        assert!(!query.contains("검색해봐"));
    }

    #[test]
    fn additional_network_requests_share_the_global_budget() {
        let budget = WebResearchBudget {
            max_steps: 8,
            max_searches: 8,
            max_query_revisions: 8,
            ..WebResearchBudget::default()
        };
        let mut research = WebResearchSession::new(budget);
        for query in ["one", "two", "three", "four", "five", "six"] {
            assert!(matches!(
                research.admit(search(query), None, Duration::ZERO),
                WebResearchAdmission::Execute(_)
            ));
        }
        assert_eq!(
            research.admit(search("seven"), None, Duration::ZERO),
            WebResearchAdmission::Stop(WebResearchTerminal::BudgetReached(
                WebResearchLimit::NetworkRequests
            ))
        );
    }

    #[test]
    fn optional_search_fallback_reservation_stays_within_the_network_budget() {
        let budget = WebResearchBudget {
            max_network_requests: 2,
            ..WebResearchBudget::default()
        };
        let mut research = WebResearchSession::new(budget);

        assert!(matches!(
            research.admit(search("primary"), None, Duration::ZERO),
            WebResearchAdmission::Execute(_)
        ));
        assert!(research.reserve_optional_network_request(Duration::ZERO));
        assert!(!research.reserve_optional_network_request(Duration::ZERO));
    }

    #[test]
    fn complete_is_an_explicit_terminal_state() {
        let mut research = WebResearchSession::default();
        assert_eq!(research.complete(), WebResearchTerminal::Complete);
        assert_eq!(
            research.admit(search("after completion"), None, Duration::ZERO),
            WebResearchAdmission::Stop(WebResearchTerminal::Complete)
        );
    }
}
