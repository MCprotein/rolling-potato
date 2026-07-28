mod context;
mod sanitize;

use context::{prepend_missing_years, relevant_context};
use sanitize::{
    clean_search_phrase, compact, has_topic, is_self_contained, nonempty_bounded, semantic_terms,
};

pub(crate) fn contextualize_search_input(
    proposed_query: &str,
    current_request: &str,
    prior_user_requests: &[&str],
) -> Option<String> {
    let proposed = clean_search_phrase(proposed_query);
    let current = clean_search_phrase(current_request);
    let needs_context = !is_self_contained(&current);
    let context = if needs_context {
        relevant_context(prior_user_requests)
    } else {
        Vec::new()
    };
    if needs_context && context.is_empty() {
        return None;
    }
    if !projects_to_user_context(proposed_query, current_request, &context) {
        return None;
    }
    if !needs_context {
        return nonempty_bounded(&proposed).or_else(|| nonempty_bounded(proposed_query));
    }

    if is_self_contained(&proposed) {
        return nonempty_bounded(&prepend_missing_years(&proposed, &context));
    }

    let mut fragments = context;
    if has_topic(&proposed) && !fragments.iter().any(|fragment| fragment == &proposed) {
        fragments.push(proposed);
    }
    nonempty_bounded(&fragments.join(" "))
}

fn projects_to_user_context(
    proposed_query: &str,
    current_request: &str,
    relevant_context: &[String],
) -> bool {
    let proposed = proposed_query.trim().to_lowercase();
    if proposed.is_empty() {
        return false;
    }
    if current_request.to_lowercase().contains(&proposed) {
        return true;
    }

    let mut user_context = relevant_context.join("\n");
    user_context.push('\n');
    user_context.push_str(current_request);
    let compact_context = compact(&user_context.to_lowercase());
    let terms = semantic_terms(&proposed);
    !terms.is_empty()
        && terms
            .iter()
            .all(|term| compact_context.contains(&compact(term)))
}

#[cfg(test)]
mod tests;
