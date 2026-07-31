use super::*;

#[test]
fn typed_tool_activity_is_restored_without_replaying_the_tool() {
    with_memory_fixture("tool-activity-resume", || {
        let identity = ledger::validated_current_identity().unwrap();
        let owner = transcript_owner(&identity);
        let activity = ConversationToolActivity::bounded(
            "web-step-1",
            ConversationToolName::Search,
            "2026 Rust stable release",
            ConversationToolStatus::Succeeded,
            ["source-rust".to_string()],
        );
        transcript::record_session_turn(
            &owner,
            "evidence",
            "tool-activity-1",
            &event_codec::render_tool_activity_event(&activity),
            &[],
        )
        .unwrap();

        let restored = load().unwrap();
        assert_eq!(restored.tool_activities(), [activity]);
        assert!(restored.turns().is_empty());
        assert!(restored.web_grounding().is_empty());
    });
}

#[test]
fn recorded_tool_activity_survives_exchange_and_cancel_without_synthetic_turns() {
    with_memory_fixture("recorded-tool-activity", || {
        let mut memory = load().unwrap();
        let succeeded = ConversationToolActivity::bounded(
            "web-success",
            ConversationToolName::Search,
            "Rust stable",
            ConversationToolStatus::Succeeded,
            ["source-rust".to_string()],
        );
        record_exchange_with_tool_activities(
            &mut memory,
            "Rust stable 찾아줘",
            "찾았습니다. [source-rust]",
            &[],
            std::slice::from_ref(&succeeded),
        )
        .unwrap();
        let cancelled = ConversationToolActivity::bounded(
            "web-cancelled",
            ConversationToolName::Open,
            "https://example.com/slow",
            ConversationToolStatus::Cancelled,
            [],
        );
        record_tool_activities(&mut memory, std::slice::from_ref(&cancelled)).unwrap();

        let restored = load().unwrap();
        assert_eq!(restored.tool_activities(), [succeeded, cancelled]);
        assert_eq!(restored.turns().len(), 2);
        assert_eq!(restored.turns()[0].content, "Rust stable 찾아줘");
        assert_eq!(restored.turns()[1].content, "찾았습니다. [source-rust]");
    });
}
