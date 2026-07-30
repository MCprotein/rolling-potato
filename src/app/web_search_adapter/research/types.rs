use std::time::Duration;

use crate::foundation::error::AppError;

pub(super) const MAX_TOOL_INPUT_CHARS: usize = 512;

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

    pub(super) fn needs_network(&self) -> bool {
        matches!(self, Self::Search { .. } | Self::Open { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WebResearchBudget {
    pub(super) max_steps: u8,
    pub(super) max_searches: u8,
    pub(super) max_opens: u8,
    pub(super) max_query_revisions: u8,
    pub(super) max_finds_per_document: u8,
    pub(super) max_network_requests: u8,
    pub(super) max_evidence_bytes: usize,
    pub(super) max_elapsed: Duration,
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
        }
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

pub(super) fn valid_step(step: &WebResearchStep) -> bool {
    let input = step.input().trim();
    !input.is_empty()
        && !input.contains(['\r', '\n'])
        && input.chars().count() <= MAX_TOOL_INPUT_CHARS
        && match step {
            WebResearchStep::Open { url } => url.starts_with("https://"),
            WebResearchStep::Search { .. } | WebResearchStep::Find { .. } => true,
        }
}

pub(super) fn bounded_input(input: &str) -> String {
    input.trim().chars().take(MAX_TOOL_INPUT_CHARS).collect()
}
