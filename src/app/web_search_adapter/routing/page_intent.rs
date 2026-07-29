use super::super::research::WebResearchStep;

pub(crate) fn route_current_page_find(
    request: &str,
    has_current_page: bool,
) -> Option<WebResearchStep> {
    if !has_current_page {
        return None;
    }
    let request = request.trim();
    let lower = request.to_ascii_lowercase();
    if !has_page_scope(request, &lower) || !has_find_action(request, &lower) {
        return None;
    }
    super::text::best_query_term(request).map(|query| WebResearchStep::Find { query })
}

fn has_page_scope(request: &str, lower: &str) -> bool {
    ["이 페이지", "현재 페이지", "열린 페이지", "방금 연 페이지"]
        .iter()
        .any(|signal| request.contains(signal))
        || ["this page", "current page", "opened page"]
            .iter()
            .any(|signal| lower.contains(signal))
}

fn has_find_action(request: &str, lower: &str) -> bool {
    ["찾아", "검색"]
        .iter()
        .any(|signal| request.contains(signal))
        || ["find ", "locate ", "search "]
            .iter()
            .any(|signal| lower.contains(signal))
}
