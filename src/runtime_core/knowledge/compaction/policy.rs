//! Model-window-aware compaction planning and source bounding.

use super::recent_tail::select_recent_tail;
use super::token_budget::{estimate_tokens, truncate_head_and_tail_to_tokens};

const AUTO_TRIGGER_PERCENT: usize = 75;
const POST_COMPACT_TARGET_PERCENT: usize = 40;
const MIN_RECENT_EXCHANGES: usize = 2;
const MAX_RECENT_EXCHANGES: usize = 8;
const MIN_RECENT_TAIL_TOKENS: usize = 512;
const MAX_RECENT_TAIL_TOKENS: usize = 16_384;
const MIN_SUMMARY_OUTPUT_TOKENS: usize = 192;
const MAX_SUMMARY_OUTPUT_TOKENS: usize = 768;
const MAX_SUMMARY_RECORD_TOKENS: usize = 1_200;
pub(super) const MAX_TOOL_SUMMARY_TOKENS: usize = 256;
pub(super) const RECORD_OVERHEAD_TOKENS: usize = 8;
pub(super) const MAX_RECENT_RECORDS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompactionMode {
    Automatic,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompactionPolicy {
    pub context_limit_tokens: usize,
    pub auto_trigger_tokens: usize,
    pub post_compact_target_tokens: usize,
    pub recent_tail_budget_tokens: usize,
    pub recent_exchange_limit: usize,
    pub summary_output_budget_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompactionRecord {
    pub record_id: String,
    pub kind: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompactionPlan {
    pub should_compact: bool,
    pub estimated_tokens_before: usize,
    pub source_record_count: usize,
    pub boundary_record_id: Option<String>,
    pub summary_source: Vec<CompactionRecord>,
    pub recent_records: Vec<CompactionRecord>,
    pub source_records_dropped: usize,
}

impl CompactionPolicy {
    pub(crate) fn for_context_limit(context_limit_tokens: usize) -> Self {
        let context_limit_tokens = context_limit_tokens.max(1);
        let auto_trigger_tokens = percent(context_limit_tokens, AUTO_TRIGGER_PERCENT).max(1);
        let post_compact_target_tokens =
            percent(context_limit_tokens, POST_COMPACT_TARGET_PERCENT).max(1);
        let recent_tail_budget_tokens = percent(context_limit_tokens, 25)
            .clamp(MIN_RECENT_TAIL_TOKENS, MAX_RECENT_TAIL_TOKENS)
            .min(post_compact_target_tokens);
        let recent_exchange_limit =
            (context_limit_tokens / 16_384).clamp(MIN_RECENT_EXCHANGES, MAX_RECENT_EXCHANGES);
        let summary_output_budget_tokens = percent(context_limit_tokens, 10)
            .clamp(MIN_SUMMARY_OUTPUT_TOKENS, MAX_SUMMARY_OUTPUT_TOKENS)
            .min(
                post_compact_target_tokens
                    .saturating_sub(recent_tail_budget_tokens)
                    .max(1),
            );
        Self {
            context_limit_tokens,
            auto_trigger_tokens,
            post_compact_target_tokens,
            recent_tail_budget_tokens,
            recent_exchange_limit,
            summary_output_budget_tokens,
        }
    }

    pub(crate) fn plan_with_observed_tokens(
        &self,
        mode: CompactionMode,
        records: &[CompactionRecord],
        observed_context_tokens: Option<usize>,
    ) -> CompactionPlan {
        let estimated_tokens_before = records
            .iter()
            .map(record_token_cost)
            .sum::<usize>()
            .max(observed_context_tokens.unwrap_or(0));
        let recent_tail = select_recent_tail(
            records,
            self.recent_tail_budget_tokens,
            self.recent_exchange_limit,
        );
        let source = &records[..recent_tail.source_end];
        let recent_records = recent_tail.records;
        let should_compact = !source.is_empty()
            && (mode == CompactionMode::Manual
                || estimated_tokens_before >= self.auto_trigger_tokens);
        let summary_budget = self
            .context_limit_tokens
            .saturating_sub(self.summary_output_budget_tokens)
            .saturating_sub(recent_records.iter().map(record_token_cost).sum::<usize>())
            .max(1);
        let (summary_source, source_records_dropped) = if should_compact {
            bounded_summary_source(source, summary_budget)
        } else {
            (Vec::new(), 0)
        };
        CompactionPlan {
            should_compact,
            estimated_tokens_before,
            source_record_count: source.len(),
            boundary_record_id: source.last().map(|record| record.record_id.clone()),
            summary_source,
            recent_records,
            source_records_dropped,
        }
    }
}

fn bounded_summary_source(
    records: &[CompactionRecord],
    budget: usize,
) -> (Vec<CompactionRecord>, usize) {
    let mut selected = Vec::new();
    let mut remaining = budget;
    for record in records.iter().rev() {
        if remaining <= RECORD_OVERHEAD_TOKENS {
            break;
        }
        let per_record_budget = if matches!(record.kind.as_str(), "tool" | "evidence") {
            MAX_TOOL_SUMMARY_TOKENS
        } else {
            MAX_SUMMARY_RECORD_TOKENS
        }
        .min(remaining.saturating_sub(RECORD_OVERHEAD_TOKENS));
        let mut bounded = record.clone();
        bounded.content = if matches!(record.kind.as_str(), "tool" | "evidence") {
            let content = truncate_head_and_tail_to_tokens(&record.content, per_record_budget);
            format!("[untrusted {} data, compacted]\n{}", record.kind, content)
        } else {
            truncate_head_and_tail_to_tokens(&record.content, per_record_budget)
        };
        let cost = record_token_cost(&bounded);
        if bounded.content.is_empty() || cost > remaining {
            continue;
        }
        remaining -= cost;
        selected.push(bounded);
    }
    selected.reverse();
    let dropped = records.len().saturating_sub(selected.len());
    (selected, dropped)
}

pub(super) fn record_token_cost(record: &CompactionRecord) -> usize {
    RECORD_OVERHEAD_TOKENS + estimate_tokens(&record.kind) + estimate_tokens(&record.content)
}

fn percent(value: usize, percent: usize) -> usize {
    value.saturating_mul(percent) / 100
}
