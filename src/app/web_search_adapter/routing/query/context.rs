use super::{clean_search_phrase, compact, has_topic, is_self_contained};

const MAX_CONTEXT_TURNS: usize = 3;

pub(super) fn relevant_context(prior_user_requests: &[&str]) -> Vec<String> {
    let mut fragments = Vec::new();
    let mut found_base = false;
    for request in prior_user_requests.iter().rev().take(MAX_CONTEXT_TURNS) {
        let fragment = clean_search_phrase(request);
        if !has_topic(&fragment) {
            continue;
        }
        if contains_private_context_marker(&fragment) {
            return Vec::new();
        }
        found_base = is_self_contained(&fragment);
        fragments.push(fragment);
        if found_base {
            break;
        }
    }
    if !found_base {
        return Vec::new();
    }
    fragments.reverse();
    fragments
}

pub(super) fn prepend_missing_years(proposed: &str, context: &[String]) -> String {
    let compact_proposed = compact(proposed);
    let mut missing = context
        .iter()
        .flat_map(|fragment| year_terms(fragment))
        .filter(|year| !compact_proposed.contains(year))
        .collect::<Vec<_>>();
    missing.sort_unstable();
    missing.dedup();
    if missing.is_empty() {
        proposed.to_string()
    } else {
        format!("{} {proposed}", missing.join(" "))
    }
}

fn year_terms(value: &str) -> Vec<&str> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter_map(|term| {
            let digits = term.trim_end_matches(|character: char| !character.is_ascii_digit());
            (digits.len() == 4 && digits.chars().all(|character| character.is_ascii_digit()))
                .then_some(digits)
        })
        .collect()
}

fn contains_private_context_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "고객 코드",
        "고객 번호",
        "계정 번호",
        "비밀번호",
        "인증 정보",
        "기억해",
        "remember this",
        "customer code",
        "account number",
        "password",
        "credential",
        "private",
        "secret",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}
