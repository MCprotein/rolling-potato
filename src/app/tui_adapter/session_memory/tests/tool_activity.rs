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

#[test]
fn ordered_search_open_find_trace_is_restored_without_replay_turns() {
    with_memory_fixture("ordered-web-trace-resume", || {
        let mut memory = load().unwrap();
        let source_ids = ["source-rust".to_string()];
        let activities = vec![
            ConversationToolActivity::bounded(
                "search-step",
                ConversationToolName::Search,
                "Rust stable release",
                ConversationToolStatus::Succeeded,
                source_ids.clone(),
            ),
            ConversationToolActivity::bounded(
                "open-step",
                ConversationToolName::Open,
                "https://example.com/rust",
                ConversationToolStatus::Succeeded,
                source_ids.clone(),
            ),
            ConversationToolActivity::bounded(
                "find-step",
                ConversationToolName::Find,
                "release",
                ConversationToolStatus::Succeeded,
                source_ids,
            ),
        ];
        record_tool_activities(&mut memory, &activities).unwrap();

        let restored = load().unwrap();
        assert_eq!(restored.tool_activities(), activities);
        assert!(restored.turns().is_empty());
        assert!(restored.web_grounding().is_empty());
    });
}

#[test]
fn local_tool_activity_names_render_and_parse_round_trip() {
    let tools = [
        ConversationToolName::ReadFile,
        ConversationToolName::ListDirectory,
        ConversationToolName::SearchRepository,
        ConversationToolName::RunReadOnlyCommand,
    ];

    for (index, tool) in tools.into_iter().enumerate() {
        let activity = ConversationToolActivity::bounded(
            format!("local-step-{index}"),
            tool,
            "src/app",
            ConversationToolStatus::Succeeded,
            [],
        );
        let rendered = event_codec::render_tool_activity_event(&activity);
        let Some(event_codec::ConversationEvent::ToolActivity(parsed)) =
            event_codec::parse_conversation_event(&rendered)
        else {
            panic!("rendered local tool activity must parse");
        };

        assert_eq!(parsed, activity);
    }
}

#[test]
fn legacy_web_tool_activity_record_still_parses() {
    let legacy = r#"{"schema_version":1,"event_type":"tool_activity","execution_id":"web-step-1","tool":"web_search","input":"Rust stable","status":"succeeded","source_ids":["source-rust"]}"#;

    let Some(event_codec::ConversationEvent::ToolActivity(parsed)) =
        event_codec::parse_conversation_event(legacy)
    else {
        panic!("legacy web tool activity must parse");
    };

    assert_eq!(parsed.execution_id, "web-step-1");
    assert_eq!(parsed.tool, ConversationToolName::Search);
    assert_eq!(parsed.input, "Rust stable");
    assert_eq!(parsed.status, ConversationToolStatus::Succeeded);
    assert_eq!(parsed.source_ids, ["source-rust"]);
}
