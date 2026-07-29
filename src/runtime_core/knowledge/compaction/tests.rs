use super::policy::MAX_TOOL_SUMMARY_TOKENS;
use super::*;

fn record(index: usize, kind: &str, content: impl Into<String>) -> CompactionRecord {
    CompactionRecord {
        record_id: format!("record-{index}"),
        kind: kind.to_string(),
        content: content.into(),
    }
}

#[test]
fn small_model_policy_triggers_at_seventy_five_percent() {
    let policy = CompactionPolicy::for_context_limit(4_096);
    assert_eq!(policy.auto_trigger_tokens, 3_072);
    assert_eq!(policy.post_compact_target_tokens, 1_638);
    assert_eq!(policy.recent_tail_budget_tokens, 1_024);
    assert_eq!(policy.recent_exchange_limit, 2);
    assert_eq!(policy.summary_output_budget_tokens, 409);
}

#[test]
fn large_model_policy_expands_recent_memory_without_changing_thresholds() {
    let policy = CompactionPolicy::for_context_limit(131_072);

    assert_eq!(policy.auto_trigger_tokens, 98_304);
    assert_eq!(policy.post_compact_target_tokens, 52_428);
    assert_eq!(policy.recent_tail_budget_tokens, 16_384);
    assert_eq!(policy.recent_exchange_limit, 8);
    assert_eq!(policy.summary_output_budget_tokens, 768);
}

#[test]
fn estimator_is_conservative_for_korean_and_code() {
    assert_eq!(estimate_tokens("abcd"), 2);
    assert_eq!(estimate_tokens("안녕하세요"), 4);
    assert!(estimate_tokens("fn main() { println!(\"hello\"); }") >= 8);
}

#[test]
fn automatic_plan_prunes_old_tool_data_and_keeps_two_recent_exchanges() {
    let policy = CompactionPolicy::for_context_limit(2_048);
    let mut records = vec![
        record(0, "user", "처음 목표와 지켜야 할 제약"),
        record(1, "model", "x".repeat(2_000)),
        record(2, "model", "x".repeat(2_000)),
        record(3, "tool", "secret-like tool output ".repeat(200)),
    ];
    records.extend([
        record(4, "user", "recent question one".repeat(10)),
        record(5, "model", "recent answer one".repeat(10)),
        record(6, "user", "recent question two".repeat(10)),
        record(7, "model", "recent answer two".repeat(10)),
    ]);

    let plan = policy.plan_with_observed_tokens(CompactionMode::Automatic, &records, None);

    assert!(plan.should_compact);
    assert_eq!(plan.recent_records.len(), 4);
    assert_eq!(plan.recent_records[0].record_id, "record-4");
    assert_eq!(plan.recent_records[3].record_id, "record-7");
    assert!(!plan.recent_records[0].content.contains("[compacted]"));
    let tool = plan
        .summary_source
        .iter()
        .find(|record| record.kind == "tool");
    let tool = tool.expect("recent old tool output should be included in bounded form");
    assert!(tool.content.starts_with("[untrusted tool data, compacted]"));
    assert!(estimate_tokens(&tool.content) <= MAX_TOOL_SUMMARY_TOKENS + 16);
    assert!(plan.source_records_dropped > 0);
}

#[test]
fn manual_plan_requires_an_older_head_but_not_the_auto_threshold() {
    let policy = CompactionPolicy::for_context_limit(4_096);
    let records = (0..5)
        .map(|index| record(index, "user", format!("turn {index}")))
        .collect::<Vec<_>>();

    assert!(
        !policy
            .plan_with_observed_tokens(CompactionMode::Automatic, &records, None)
            .should_compact
    );
    assert!(
        policy
            .plan_with_observed_tokens(CompactionMode::Manual, &records, None)
            .should_compact
    );
    let plan = policy.plan_with_observed_tokens(CompactionMode::Manual, &records, None);
    assert_eq!(plan.source_record_count, 3);
    assert_eq!(plan.boundary_record_id.as_deref(), Some("record-2"));
    assert!(
        !policy
            .plan_with_observed_tokens(CompactionMode::Manual, &records[..2], None)
            .should_compact
    );
}

#[test]
fn observed_compiled_context_can_trigger_when_transcript_estimate_is_smaller() {
    let policy = CompactionPolicy::for_context_limit(4_096);
    let records = (0..5)
        .map(|index| record(index, "user", format!("turn {index}")))
        .collect::<Vec<_>>();

    let plan = policy.plan_with_observed_tokens(
        CompactionMode::Automatic,
        &records,
        Some(policy.auto_trigger_tokens),
    );

    assert!(plan.should_compact);
    assert_eq!(plan.estimated_tokens_before, policy.auto_trigger_tokens);
}

#[test]
fn checkpoint_normalizes_deduplicates_and_marks_history_untrusted() {
    let mut checkpoint = CompactionCheckpoint {
        current_task: "  context   compaction 구현 ".to_string(),
        constraints: vec![
            "targeted tests only".to_string(),
            "targeted   tests only".to_string(),
            " ".to_string(),
        ],
        remaining_work: vec!["wire /compact".to_string()],
        rationale: " previous  model   discussion ".to_string(),
        ..CompactionCheckpoint::default()
    };

    checkpoint.normalize();
    let prompt = checkpoint.prompt_section();

    assert_eq!(checkpoint.current_task, "context compaction 구현");
    assert_eq!(checkpoint.constraints, ["targeted tests only"]);
    assert!(prompt.contains("untrusted historical data"));
    assert!(prompt.contains("- remaining work:\n  - wire /compact"));
    assert!(prompt.contains("- decisions:\n  - 없음"));
}

#[test]
fn checkpoint_normalization_keeps_the_newest_bounded_items() {
    let mut checkpoint = CompactionCheckpoint {
        remaining_work: (0..9).map(|index| format!("work-{index}")).collect(),
        ..CompactionCheckpoint::default()
    };

    checkpoint.normalize();

    assert_eq!(
        checkpoint.remaining_work,
        ["work-3", "work-4", "work-5", "work-6", "work-7", "work-8"]
    );
}

#[test]
fn token_truncation_honors_korean_byte_and_character_bounds() {
    let text = "한글 컨텍스트 ".repeat(2_000);

    let head = truncate_head_to_tokens(&text, 128);
    let tail = truncate_tail_to_estimated_tokens(&text, 128);

    assert!(estimate_tokens(&head) <= 128);
    assert!(estimate_tokens(&tail) <= 128);
    assert!(head.ends_with("[compacted]\n"));
    assert!(tail.starts_with("\n[compacted]"));
}
