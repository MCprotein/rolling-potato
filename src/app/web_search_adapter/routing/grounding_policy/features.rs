#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GroundingSignals {
    explicit_public_scope: bool,
    local_scope: bool,
    search_verb: bool,
    freshness: bool,
    volatile_outcome: bool,
    current_release: bool,
    empirical_comparison: bool,
}

impl GroundingSignals {
    pub(super) fn from_request(request: &str) -> Self {
        let request = request.trim();
        let lower = request.to_ascii_lowercase();
        Self {
            explicit_public_scope: has_public_web_scope(request, &lower),
            local_scope: has_local_scope(&lower),
            search_verb: has_search_verb(request, &lower),
            freshness: has_freshness_signal(request, &lower),
            volatile_outcome: asks_for_volatile_outcome(request, &lower),
            current_release: asks_for_current_version_or_release(&lower),
            empirical_comparison: asks_for_empirical_comparison(request, &lower),
        }
    }

    pub(super) fn requires_external_grounding(self) -> bool {
        if self.explicit_public_scope {
            return true;
        }
        if self.local_scope {
            return false;
        }
        self.search_verb
            || self.freshness
            || self.volatile_outcome
            || self.current_release
            || self.empirical_comparison
    }

    pub(super) fn query_kind(self) -> GroundingQueryKind {
        if self.volatile_outcome {
            GroundingQueryKind::Outcome
        } else if self.empirical_comparison {
            GroundingQueryKind::Comparison
        } else if self.current_release || self.freshness {
            GroundingQueryKind::CurrentFact
        } else {
            GroundingQueryKind::General
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GroundingQueryKind {
    General,
    CurrentFact,
    Outcome,
    Comparison,
}

fn has_public_web_scope(request: &str, lower: &str) -> bool {
    ["웹에서", "인터넷에서", "온라인에서"]
        .iter()
        .any(|signal| request.contains(signal))
        || [
            "on the web",
            "search the web",
            "search online",
            "browse online",
        ]
        .iter()
        .any(|signal| lower.contains(signal))
}

fn has_search_verb(request: &str, lower: &str) -> bool {
    [
        "검색해",
        "검색해서",
        "검색하여",
        "찾아줘",
        "찾아봐",
        "찾아보",
    ]
    .iter()
    .any(|signal| request.contains(signal))
        || ["search for", "look up", "browse for"]
            .iter()
            .any(|signal| lower.contains(signal))
}

fn has_freshness_signal(request: &str, lower: &str) -> bool {
    ["최신", "최근 뉴스", "오늘 뉴스", "실시간", "방금 발표"]
        .iter()
        .any(|signal| request.contains(signal))
        || ["latest", "breaking news", "real-time", "just announced"]
            .iter()
            .any(|signal| lower.contains(signal))
}

fn asks_for_volatile_outcome(request: &str, lower: &str) -> bool {
    let temporal = contains_year(request)
        || [
            "현재",
            "지금",
            "오늘",
            "올해",
            "이번",
            "어제",
            "current",
            "today",
            "this year",
        ]
        .iter()
        .any(|signal| lower.contains(signal));
    let outcome = [
        "우승",
        "결과",
        "점수",
        "순위",
        "당선",
        "수상",
        "발표",
        "가격",
        "환율",
        "주가",
        "날씨",
        "대통령",
        "ceo",
        "winner",
        "won",
        "result",
        "score",
        "ranking",
        "elected",
        "price",
        "weather",
    ]
    .iter()
    .any(|signal| lower.contains(signal));
    temporal && outcome
}

fn asks_for_current_version_or_release(lower: &str) -> bool {
    let temporal = ["현재", "지금", "최신", "current", "latest"]
        .iter()
        .any(|signal| lower.contains(signal));
    let release = ["버전", "릴리스", "출시", "version", "release"]
        .iter()
        .any(|signal| lower.contains(signal));
    temporal && release
}

fn asks_for_empirical_comparison(request: &str, lower: &str) -> bool {
    let comparison = [
        " vs ",
        "대비",
        "비교",
        "차이",
        "더 좋아",
        "더 나아",
        "versus",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
        || lower.split_whitespace().any(|term| term == "vs");
    let evidence = [
        "성능",
        "벤치마크",
        "속도",
        "정확도",
        "가격",
        "사양",
        "컨텍스트",
        "메모리",
        "ram",
        "performance",
        "benchmark",
        "speed",
        "accuracy",
        "latency",
        "context",
        "memory",
    ]
    .iter()
    .any(|signal| lower.contains(signal));
    comparison
        && evidence
        && request
            .chars()
            .filter(|character| character.is_alphanumeric())
            .count()
            >= 6
}

fn contains_year(request: &str) -> bool {
    request
        .split(|character: char| !character.is_ascii_digit())
        .any(|term| {
            term.len() == 4
                && term.starts_with("20")
                && term.chars().all(|character| character.is_ascii_digit())
        })
}

fn has_local_scope(lower: &str) -> bool {
    [
        "현재 파일",
        "이 파일",
        "이 코드",
        "현재 코드",
        "이 저장소",
        "현재 저장소",
        "이 프로젝트",
        "현재 프로젝트",
        "로컬 파일",
        "로컬 코드",
        "src/",
        "tests/",
        "current file",
        "this file",
        "this code",
        "current code",
        "this repository",
        "current repository",
        "this repo",
        "current repo",
        "local file",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
}
