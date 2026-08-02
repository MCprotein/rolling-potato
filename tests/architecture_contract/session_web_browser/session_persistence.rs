#[test]
fn session_memory_review_fixes_keep_separate_bounded_owners() {
    let tui_runtime = fs::read_to_string("src/app/tui_adapter/runtime.rs").unwrap();
    let tui_runtime_port = fs::read_to_string("src/app/tui_adapter/runtime/port.rs").unwrap();
    let tui_request = fs::read_to_string("src/app/tui_adapter/runtime/request.rs").unwrap();
    let tui_status = fs::read_to_string("src/app/tui_adapter/runtime/status.rs").unwrap();
    let tui_status_tests =
        fs::read_to_string("src/app/tui_adapter/runtime/status/tests.rs").unwrap();
    let session_memory = fs::read_to_string("src/app/tui_adapter/session_memory.rs").unwrap();
    let session_event_codec =
        fs::read_to_string("src/app/tui_adapter/session_memory/event_codec.rs").unwrap();
    let session_restoration =
        fs::read_to_string("src/app/tui_adapter/session_memory/restoration.rs").unwrap();
    let session_tests = fs::read_to_string("src/app/tui_adapter/session_memory/tests.rs").unwrap();
    let session_restoration_tests =
        fs::read_to_string("src/app/tui_adapter/session_memory/tests/restoration.rs").unwrap();
    let intent_tests = fs::read_to_string("src/app/intent_adapter/tests.rs").unwrap();
    let prompt_budget_tests =
        fs::read_to_string("src/app/intent_adapter/tests/prompt_budget.rs").unwrap();
    let context = fs::read_to_string("src/runtime_core/knowledge/context.rs").unwrap();
    let context_assembly =
        fs::read_to_string("src/runtime_core/knowledge/context/assembly.rs").unwrap();
    let context_budget =
        fs::read_to_string("src/runtime_core/knowledge/context/budget.rs").unwrap();
    let context_types = fs::read_to_string("src/runtime_core/knowledge/context/types.rs").unwrap();
    let compaction = fs::read_to_string("src/runtime_core/knowledge/compaction.rs").unwrap();
    let compaction_checkpoint =
        fs::read_to_string("src/runtime_core/knowledge/compaction/checkpoint.rs").unwrap();
    let compaction_policy =
        fs::read_to_string("src/runtime_core/knowledge/compaction/policy.rs").unwrap();
    let recent_tail =
        fs::read_to_string("src/runtime_core/knowledge/compaction/recent_tail.rs").unwrap();
    let compaction_tests =
        fs::read_to_string("src/runtime_core/knowledge/compaction/tests.rs").unwrap();
    let token_budget =
        fs::read_to_string("src/runtime_core/knowledge/compaction/token_budget.rs").unwrap();
    let native_terminal = fs::read_to_string("tests/surfaces/native_terminal.rs").unwrap();

    assert!(session_memory.contains("#[path = \"session_memory/tests.rs\"]"));
    assert!(session_memory
        .lines()
        .any(|line| line == "mod event_codec;"));
    assert!(session_memory
        .lines()
        .any(|line| line == "mod restoration;"));
    assert!(session_event_codec.contains("fn parse_conversation_event("));
    assert!(session_restoration.contains("fn load_for_session("));
    assert!(session_tests.contains("#[path = \"tests/restoration.rs\"]"));
    assert!(session_tests
        .lines()
        .any(|line| line == "mod restoration_tests;"));
    assert!(session_restoration_tests
        .contains("fn web_grounding_is_bounded_and_restored_for_followups_after_resume("));
    assert!(intent_tests.contains("#[path = \"tests/prompt_budget.rs\"]"));
    for owner in ["checkpoint", "policy", "recent_tail", "token_budget"] {
        assert!(
            compaction
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "compaction facade does not register {owner}"
        );
    }

    for responsibility in [
        "fn reset_is_a_unique_causal_head_for_repeated_questions(",
        "fn reset_discards_an_orphan_user_before_a_later_model_record(",
        "fn coding_exchange_is_canonical_and_prompt_history_keeps_budgetable_pairs(",
    ] {
        assert!(
            session_tests.contains(responsibility),
            "session-memory regression owner is missing: {responsibility}"
        );
        assert!(
            !session_memory.contains(responsibility),
            "session-memory production owner contains regression test: {responsibility}"
        );
    }
    assert!(tui_runtime.lines().any(|line| line == "mod port;"));
    assert!(tui_runtime_port.contains("session_memory::record_exchange_with_tool_activities("));
    assert!(!tui_request.contains("TranscriptOwner"));
    assert!(tui_status.contains("conversation::estimate_context_tokens("));
    assert!(tui_status.contains("resolve_context_tokens(latest_context_tokens"));
    assert!(!tui_status.contains("fn estimate_retained_tokens("));
    assert!(tui_status.contains("mod tests;"));
    assert!(tui_status_tests.contains("fn backend_observation_precedes_prompt_projection("));
    assert!(
        !native_terminal.contains("confirm_picker(&mut terminal, \"세션 선택 확인\")"),
        "session resume는 workflow dispatch fault probe로 재사용하면 안 됩니다."
    );

    for responsibility in [
        "fn imported_skill_instructions_are_bounded_by_runtime_contract(",
        "fn agent_loop_prompt_bounds_resume_and_sources_to_the_active_runtime_window(",
    ] {
        assert!(
            prompt_budget_tests.contains(responsibility),
            "agent prompt regression owner is missing: {responsibility}"
        );
        assert!(
            !intent_tests.contains(responsibility),
            "intent regression facade contains prompt-budget test: {responsibility}"
        );
    }
    for (owner, responsibility) in [
        (&context_budget, "struct AgentPromptBudget"),
        (&context_types, "struct AgentPromptParts"),
        (&context_assembly, "fn assemble_agent_prompt("),
    ] {
        assert!(
            owner.contains(responsibility),
            "context owner is missing agent prompt policy: {responsibility}"
        );
        assert!(
            !context.contains(responsibility),
            "context facade still owns agent prompt policy: {responsibility}"
        );
    }

    for responsibility in [
        "fn select_recent_tail(",
        "fn exchange_ranges(",
        "fn bounded_single_exchange(",
    ] {
        assert!(
            recent_tail.contains(responsibility),
            "recent-tail owner is missing: {responsibility}"
        );
        assert!(
            !compaction.contains(responsibility),
            "compaction facade still owns recent-tail policy: {responsibility}"
        );
    }
    for (owner, responsibilities) in [
        (
            compaction_checkpoint.as_str(),
            ["struct CompactionCheckpoint", "fn prompt_section"].as_slice(),
        ),
        (
            compaction_policy.as_str(),
            ["struct CompactionPolicy", "fn plan_with_observed_tokens"].as_slice(),
        ),
        (
            token_budget.as_str(),
            ["fn estimate_tokens", "fn truncate_to_token_budget"].as_slice(),
        ),
    ] {
        for responsibility in responsibilities {
            assert!(
                owner.contains(responsibility),
                "compaction responsibility owner is missing {responsibility}"
            );
            assert!(
                !compaction.contains(responsibility),
                "compaction facade still owns {responsibility}"
            );
        }
    }

    assert!(session_memory.lines().count() < 225);
    assert!(session_tests.lines().count() < 225);
    assert!(session_event_codec.lines().count() < 125);
    assert!(session_restoration.lines().count() < 125);
    assert!(session_restoration_tests.lines().count() < 175);
    assert!(intent_tests.lines().count() < 325);
    assert!(prompt_budget_tests.lines().count() < 175);
    assert!(compaction.lines().count() < 50);
    assert!(compaction_checkpoint.lines().count() < 125);
    assert!(compaction_policy.lines().count() < 200);
    assert!(recent_tail.lines().count() < 350);
    assert!(compaction_tests.lines().count() < 225);
    assert!(token_budget.lines().count() < 150);
    assert!(tui_status.lines().count() < 150);
    assert!(tui_status_tests.lines().count() < 125);
}
