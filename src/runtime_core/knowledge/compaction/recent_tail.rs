//! Complete-exchange selection and bounding for the post-compaction recent tail.

use std::collections::BTreeSet;

use super::{
    estimate_tokens, record_token_cost, truncate_tail_to_estimated_tokens, CompactionRecord,
    MAX_RECENT_RECORDS, RECORD_OVERHEAD_TOKENS,
};

#[derive(Debug, Clone, Copy)]
struct ExchangeRange {
    start: usize,
    end: usize,
}

#[derive(Debug)]
pub(super) struct RecentTail {
    pub(super) source_end: usize,
    pub(super) records: Vec<CompactionRecord>,
}

pub(super) fn select_recent_tail(
    records: &[CompactionRecord],
    budget: usize,
    max_exchanges: usize,
) -> RecentTail {
    let exchanges = exchange_ranges(records);
    let mut selected_start = records.len();
    let mut selected_cost = 0usize;
    let mut selected_records = 0usize;
    let mut selected_exchanges = 0usize;

    for exchange in exchanges.iter().rev() {
        if selected_exchanges == max_exchanges {
            break;
        }
        let exchange_records = &records[exchange.start..exchange.end];
        let exchange_cost = exchange_records
            .iter()
            .map(record_token_cost)
            .sum::<usize>();
        let exceeds_budget = selected_cost.saturating_add(exchange_cost) > budget;
        let exceeds_record_limit =
            selected_records.saturating_add(exchange_records.len()) > MAX_RECENT_RECORDS;
        if selected_exchanges > 0 && (exceeds_budget || exceeds_record_limit) {
            break;
        }
        selected_start = exchange.start;
        selected_cost = selected_cost.saturating_add(exchange_cost);
        selected_records = selected_records.saturating_add(exchange_records.len());
        selected_exchanges += 1;
    }

    if selected_exchanges == 0 {
        return RecentTail {
            source_end: records.len(),
            records: Vec::new(),
        };
    }

    let selected = &records[selected_start..];
    let bounded = if selected.len() > MAX_RECENT_RECORDS || selected_cost > budget {
        bounded_single_exchange(selected, budget)
    } else {
        selected.to_vec()
    };
    RecentTail {
        source_end: if bounded.is_empty() {
            records.len()
        } else {
            selected_start
        },
        records: bounded,
    }
}

fn exchange_ranges(records: &[CompactionRecord]) -> Vec<ExchangeRange> {
    let starts = records
        .iter()
        .enumerate()
        .filter(|(_, record)| record.kind == "user")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    starts
        .iter()
        .copied()
        .enumerate()
        .map(|(position, start)| ExchangeRange {
            start,
            end: starts.get(position + 1).copied().unwrap_or(records.len()),
        })
        .collect()
}

fn bounded_single_exchange(records: &[CompactionRecord], budget: usize) -> Vec<CompactionRecord> {
    let mut record_limit = records.len().min(MAX_RECENT_RECORDS);
    let mut bounded = essential_exchange_records(records, record_limit);
    while fixed_record_cost(&bounded) > budget && record_limit > 2 {
        record_limit -= 1;
        bounded = essential_exchange_records(records, record_limit);
    }
    if fixed_record_cost(&bounded) > budget {
        return Vec::new();
    }

    let full_cost = records.iter().map(record_token_cost).sum::<usize>();
    if full_cost <= budget && records.len() <= MAX_RECENT_RECORDS {
        return bounded;
    }

    let fixed_cost = fixed_record_cost(&bounded);
    let content_budget = budget - fixed_cost;
    let per_record_budget = content_budget / bounded.len().max(1);
    for record in &mut bounded {
        record.content = truncate_tail_to_estimated_tokens(&record.content, per_record_budget);
    }
    bounded
}

fn essential_exchange_records(records: &[CompactionRecord], limit: usize) -> Vec<CompactionRecord> {
    if records.len() <= limit {
        return records.to_vec();
    }
    let mut selected = BTreeSet::new();
    selected.insert(0);
    if let Some(model_index) = records.iter().rposition(|record| record.kind == "model") {
        selected.insert(model_index);
    }
    for index in (1..records.len()).rev() {
        if selected.len() == limit {
            break;
        }
        selected.insert(index);
    }
    selected
        .into_iter()
        .map(|index| records[index].clone())
        .collect()
}

fn fixed_record_cost(records: &[CompactionRecord]) -> usize {
    records
        .iter()
        .map(|record| RECORD_OVERHEAD_TOKENS + estimate_tokens(&record.kind))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_core::knowledge::compaction::{CompactionMode, CompactionPolicy};

    fn record(index: usize, kind: &str, content: impl Into<String>) -> CompactionRecord {
        CompactionRecord {
            record_id: format!("record-{index}"),
            kind: kind.to_string(),
            content: content.into(),
        }
    }

    #[test]
    fn oversized_latest_exchange_keeps_user_and_model_records_within_budget() {
        let policy = CompactionPolicy::for_context_limit(4_096);
        let records = vec![
            record(0, "user", "older request"),
            record(1, "model", "older response"),
            record(2, "user", "최신 질문 ".repeat(4_000)),
            record(3, "model", "최신 답변 ".repeat(4_000)),
        ];

        let plan = policy.plan_with_observed_tokens(CompactionMode::Manual, &records, None);

        assert!(plan.should_compact);
        assert_eq!(
            plan.recent_records
                .iter()
                .map(|record| record.kind.as_str())
                .collect::<Vec<_>>(),
            ["user", "model"]
        );
        assert!(
            plan.recent_records
                .iter()
                .map(record_token_cost)
                .sum::<usize>()
                <= policy.recent_tail_budget_tokens
        );
    }

    #[test]
    fn record_ceiling_drops_whole_older_exchanges() {
        let policy = CompactionPolicy::for_context_limit(131_072);
        let mut records = vec![
            record(0, "user", "old request"),
            record(1, "model", "old response"),
        ];
        records.push(record(2, "user", "middle request"));
        for index in 3..43 {
            records.push(record(index, "tool", "middle tool"));
        }
        records.push(record(43, "model", "middle response"));
        records.push(record(44, "user", "latest request"));
        for index in 45..85 {
            records.push(record(index, "tool", "latest tool"));
        }
        records.push(record(85, "model", "latest response"));

        let plan = policy.plan_with_observed_tokens(CompactionMode::Manual, &records, None);

        assert_eq!(plan.source_record_count, 44);
        assert_eq!(plan.boundary_record_id.as_deref(), Some("record-43"));
        assert_eq!(plan.recent_records.len(), 42);
        assert_eq!(plan.recent_records.first().unwrap().record_id, "record-44");
        assert_eq!(plan.recent_records.last().unwrap().record_id, "record-85");
        assert!(plan
            .recent_records
            .iter()
            .all(|record| record.record_id != "record-43"));
    }

    #[test]
    fn oversized_single_exchange_preserves_its_user_model_pair() {
        let policy = CompactionPolicy::for_context_limit(131_072);
        let mut records = vec![
            record(0, "user", "old request"),
            record(1, "model", "old response"),
            record(2, "user", "latest request"),
        ];
        for index in 3..83 {
            records.push(record(index, "tool", "latest tool"));
        }
        records.push(record(83, "model", "latest response"));

        let plan = policy.plan_with_observed_tokens(CompactionMode::Manual, &records, None);

        assert_eq!(plan.source_record_count, 2);
        assert!(plan.recent_records.len() <= MAX_RECENT_RECORDS);
        assert_eq!(plan.recent_records.first().unwrap().record_id, "record-2");
        assert_eq!(plan.recent_records.last().unwrap().record_id, "record-83");
        assert_eq!(
            plan.recent_records
                .iter()
                .filter(|record| record.kind == "user")
                .count(),
            1
        );
        assert_eq!(
            plan.recent_records
                .iter()
                .filter(|record| record.kind == "model")
                .count(),
            1
        );
    }

    #[test]
    fn minimum_tail_budget_never_mixes_or_breaks_exchanges() {
        let policy = CompactionPolicy::for_context_limit(2_048);
        let mut records = vec![
            record(0, "user", "old request"),
            record(1, "tool", "old tool"),
            record(2, "model", "old response"),
            record(3, "user", "latest request ".repeat(2_000)),
        ];
        for index in 4..74 {
            records.push(record(index, "tool", "latest tool output ".repeat(200)));
        }
        records.push(record(74, "model", "latest response ".repeat(2_000)));

        let plan = policy.plan_with_observed_tokens(CompactionMode::Manual, &records, None);

        assert_eq!(policy.recent_tail_budget_tokens, 512);
        assert_eq!(plan.source_record_count, 3);
        assert!(plan.recent_records.len() <= MAX_RECENT_RECORDS);
        assert_eq!(plan.recent_records.first().unwrap().record_id, "record-3");
        assert!(plan
            .recent_records
            .iter()
            .all(|record| record.record_id != "record-2"));
        assert!(plan
            .recent_records
            .iter()
            .any(|record| record.record_id == "record-74"));
        assert!(
            plan.recent_records
                .iter()
                .map(record_token_cost)
                .sum::<usize>()
                <= policy.recent_tail_budget_tokens
        );
    }
}
