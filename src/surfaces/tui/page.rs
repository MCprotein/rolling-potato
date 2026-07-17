use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime_core::workflow::domain::snapshot::TuiStateSnapshot;
use crate::runtime_core::workflow::storage_compat::ledger::LedgerBinding;

use super::runtime_bridge::{
    TuiFreshness, TuiReadAuthority, TuiReadBudget, TuiReadContinuation, TuiReadPage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionStatus {
    Clear,
    Lagging,
    Unavailable,
}

pub(crate) fn bounded_budget_for(
    budget: TuiReadBudget,
    max_items: usize,
    max_chars: usize,
) -> TuiReadBudget {
    TuiReadBudget {
        max_items: budget.max_items.clamp(1, max_items),
        max_chars: budget.max_chars.clamp(1, max_chars),
    }
}

pub(crate) fn page_slice<T>(rows: Vec<T>, page: u64, items: usize) -> Vec<T> {
    let offset = page_offset(page, items);
    rows.into_iter().skip(offset).take(items).collect()
}

fn page_offset(page: u64, items: usize) -> usize {
    usize::try_from(page)
        .ok()
        .and_then(|page| page.checked_mul(items))
        .unwrap_or(usize::MAX)
}

pub(crate) fn paged_chars(text: &str, page: u64, max_chars: usize) -> (String, bool) {
    let mut chars = text.chars().skip(page_offset(page, max_chars));
    let page = chars.by_ref().take(max_chars).collect::<String>();
    (page, chars.next().is_some())
}

pub(crate) fn paged_diff(
    text: &str,
    page: u64,
    max_lines: usize,
    max_chars: usize,
) -> (String, bool) {
    let mut pages = vec![String::new()];
    let mut line_counts = vec![0_usize];
    for logical_line in text.split_inclusive('\n') {
        let mut remaining = logical_line;
        while !remaining.is_empty() {
            let index = pages.len() - 1;
            let available = max_chars.saturating_sub(pages[index].chars().count());
            if line_counts[index] == max_lines || available == 0 {
                pages.push(String::new());
                line_counts.push(0);
                continue;
            }
            let chunk = remaining.chars().take(available).collect::<String>();
            if chunk.is_empty() {
                pages.push(String::new());
                line_counts.push(0);
                continue;
            }
            let bytes = chunk.len();
            pages[index].push_str(&chunk);
            remaining = &remaining[bytes..];
            if remaining.is_empty() || chunk.ends_with('\n') {
                line_counts[index] = line_counts[index].saturating_add(1);
            }
            if !remaining.is_empty() {
                pages.push(String::new());
                line_counts.push(0);
            }
        }
    }
    let index = usize::try_from(page).unwrap_or(usize::MAX);
    let selected = pages.get(index).cloned().unwrap_or_default();
    (selected, index.saturating_add(1) < pages.len())
}

pub(crate) fn page_has_next(page: u64, items: usize, total: usize) -> bool {
    usize::try_from(page)
        .ok()
        .and_then(|page| page.checked_add(1))
        .and_then(|page| page.checked_mul(items))
        .is_some_and(|offset| offset < total)
}

pub(crate) fn page_continuation(has_next: bool, source_truncated: bool) -> TuiReadContinuation {
    if has_next {
        TuiReadContinuation::NextPage
    } else if source_truncated {
        TuiReadContinuation::Truncated
    } else {
        TuiReadContinuation::Complete
    }
}

fn ledger_page_authority(
    binding: &LedgerBinding,
    projected_events: Option<i64>,
) -> TuiReadAuthority {
    TuiReadAuthority {
        ledger_sequence: Some(binding.event_count),
        ledger_hash: Some(binding.event_hash.clone()),
        projected_sequence: projected_events.and_then(|value| u64::try_from(value).ok()),
        validated_at_ms: Some(tui_now_ms()),
        ..TuiReadAuthority::default()
    }
}

pub(crate) fn state_page_authority(
    snapshot: &TuiStateSnapshot,
    projected_events: Option<i64>,
) -> TuiReadAuthority {
    let mut authority = ledger_page_authority(&snapshot.ledger_binding, projected_events);
    authority.current_revision = Some(snapshot.current_revision);
    authority.current_hash = Some(snapshot.current_hash.clone());
    if let Some(workflow) = snapshot.active_workflow.as_ref() {
        authority.workflow_revision = Some(workflow.revision);
        authority.workflow_hash = Some(workflow.artifact_hash.clone());
    }
    authority
}

pub(crate) fn unavailable_page(
    title: &str,
    page: u64,
    budget: TuiReadBudget,
    reason: &str,
    authority: TuiReadAuthority,
    truncated: bool,
) -> TuiReadPage {
    build_page(
        title,
        vec![format!("unavailable: {reason}")],
        budget,
        page_meta(
            page,
            false,
            TuiFreshness::Unavailable,
            authority,
            if truncated {
                TuiReadContinuation::Truncated
            } else {
                TuiReadContinuation::Unavailable
            },
        ),
    )
}

fn tui_now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub(crate) struct TuiPageMeta {
    page: u64,
    has_next: bool,
    freshness: TuiFreshness,
    authority: TuiReadAuthority,
    continuation: TuiReadContinuation,
}

pub(crate) fn page_meta(
    page: u64,
    has_next: bool,
    freshness: TuiFreshness,
    authority: TuiReadAuthority,
    continuation: TuiReadContinuation,
) -> TuiPageMeta {
    TuiPageMeta {
        page,
        has_next,
        freshness,
        authority,
        continuation,
    }
}

pub(crate) fn build_page(
    title: &str,
    lines: Vec<String>,
    budget: TuiReadBudget,
    meta: TuiPageMeta,
) -> TuiReadPage {
    let TuiPageMeta {
        page,
        has_next,
        freshness,
        authority,
        mut continuation,
    } = meta;
    let mut remaining = budget.max_chars;
    let mut bounded = Vec::new();
    let total_lines = lines.len();
    for line in lines.into_iter().take(budget.max_items) {
        if remaining == 0 {
            if continuation == TuiReadContinuation::Complete {
                continuation = TuiReadContinuation::Truncated;
            }
            break;
        }
        let clipped = line.chars().take(remaining).collect::<String>();
        if clipped.chars().count() < line.chars().count()
            && continuation == TuiReadContinuation::Complete
        {
            continuation = TuiReadContinuation::Truncated;
        }
        remaining = remaining.saturating_sub(clipped.chars().count());
        bounded.push(clipped);
    }
    if total_lines > budget.max_items && continuation == TuiReadContinuation::Complete {
        continuation = TuiReadContinuation::Truncated;
    }
    TuiReadPage {
        title: title.to_string(),
        lines: bounded,
        page,
        has_previous: page > 0,
        has_next,
        freshness,
        continuation,
        authority,
    }
}

pub(crate) fn tui_read_freshness(
    canonical_events: u64,
    projected_events: Option<i64>,
    projection_status: ProjectionStatus,
) -> TuiFreshness {
    match projection_status {
        ProjectionStatus::Clear => {}
        ProjectionStatus::Lagging => return TuiFreshness::ProjectionLag,
        ProjectionStatus::Unavailable => return TuiFreshness::Unavailable,
    }
    let Some(projected_events) = projected_events else {
        return TuiFreshness::Unavailable;
    };
    match u64::try_from(projected_events) {
        Ok(projected) if projected == canonical_events => TuiFreshness::Fresh,
        Ok(_) => TuiFreshness::Stale,
        Err(_) => TuiFreshness::Unavailable,
    }
}
