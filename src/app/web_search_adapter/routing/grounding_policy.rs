const MAX_QUERY_CHARS: usize = 512;

pub(in crate::app::web_search_adapter) fn requires_external_grounding(request: &str) -> bool {
    let request = request.trim();
    if request.is_empty() {
        return false;
    }
    let lower = request.to_ascii_lowercase();
    if has_public_web_scope(request, &lower) {
        return true;
    }
    if has_local_scope(request) {
        return false;
    }
    has_search_verb(request, &lower)
        || has_freshness_signal(request, &lower)
        || asks_for_volatile_outcome(request, &lower)
        || asks_for_current_version_or_release(&lower)
        || asks_for_empirical_comparison(request, &lower)
}

pub(in crate::app::web_search_adapter) fn is_empirical_comparison_request(request: &str) -> bool {
    asks_for_empirical_comparison(request, &request.to_ascii_lowercase())
}

pub(in crate::app::web_search_adapter) fn strengthen_search_query(
    query: &str,
    request: &str,
) -> String {
    let query = query.trim();
    let request_lower = request.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();
    let mut additions = Vec::new();

    if asks_about_world_cup(request, &request_lower) {
        additions.extend(["FIFA", "World Cup"]);
    }
    if asks_for_volatile_outcome(request, &request_lower) {
        additions.extend(["공식", "official", "result"]);
    } else if asks_for_empirical_comparison(request, &request_lower) {
        additions.extend(["공식", "문서", "benchmark"]);
        if request_lower.contains("gemma") {
            additions.push("Google");
        }
        if request_lower.contains("qwen") {
            additions.push("Alibaba");
        }
    } else if asks_for_current_version_or_release(&request_lower) {
        additions.extend(["공식", "official"]);
    }

    additions.retain(|term| !query_lower.contains(&term.to_ascii_lowercase()));
    additions.dedup();
    if additions.is_empty() {
        return query.chars().take(MAX_QUERY_CHARS).collect();
    }

    let suffix = format!(" {}", additions.join(" "));
    let keep = MAX_QUERY_CHARS.saturating_sub(suffix.chars().count());
    let mut strengthened = query.chars().take(keep).collect::<String>();
    strengthened.push_str(&suffix);
    strengthened
}

fn asks_about_world_cup(request: &str, lower: &str) -> bool {
    request.contains("월드컵") || lower.contains("world cup")
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

fn has_local_scope(request: &str) -> bool {
    let lower = request.to_ascii_lowercase();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_grounding_is_narrow_and_respects_local_scope() {
        for request in [
            "2026년 월드컵 우승국가 어디냐",
            "gemma vs qwen 성능 비교해봐",
            "현재 Rust stable 버전이 뭐야?",
            "최신 llama.cpp 릴리스를 알려줘",
        ] {
            assert!(requires_external_grounding(request), "{request}");
        }
        for request in [
            "대한민국의 수도는?",
            "Gemma와 Qwen의 이름만 비교해줘",
            "현재 파일의 두 함수 성능을 비교해줘",
            "이 저장소에서 최신 release 코드를 찾아줘",
        ] {
            assert!(!requires_external_grounding(request), "{request}");
        }
        assert!(requires_external_grounding(
            "이 코드 관련 최신 Rust API를 웹에서 검색해줘"
        ));
    }

    #[test]
    fn query_strengthening_uses_standard_event_and_vendor_names() {
        let world_cup =
            strengthen_search_query("2026년 월드컵 우승국가", "2026년 월드컵 우승국가 어디냐");
        assert!(world_cup.contains("FIFA World Cup"), "{world_cup}");
        assert!(world_cup.contains("official"), "{world_cup}");

        let comparison =
            strengthen_search_query("gemma vs qwen 성능 비교", "gemma vs qwen 성능 비교해봐");
        assert!(comparison.contains("Google"), "{comparison}");
        assert!(comparison.contains("Alibaba"), "{comparison}");
        assert!(comparison.contains("benchmark"), "{comparison}");
    }
}
