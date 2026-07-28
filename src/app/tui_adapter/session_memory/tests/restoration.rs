use super::*;

#[test]
fn canonical_memory_restores_only_complete_pairs_and_honors_reset_boundaries() {
    with_memory_fixture("complete-pairs", || {
        let mut memory = load().unwrap();
        assert!(memory.turns.is_empty());

        record_exchange(&mut memory, "내 이름은 감자야", "알겠습니다.", &[]).unwrap();
        assert_eq!(memory.turns.len(), 2);
        assert_eq!(load().unwrap(), memory);

        clear(&mut memory).unwrap();
        assert!(memory.turns.is_empty());
        assert!(load().unwrap().turns.is_empty());
    });
}

#[test]
fn failed_request_is_restored_as_runtime_context_for_followup_questions() {
    with_memory_fixture("failed-request-context", || {
        let mut memory = load().unwrap();
        record_failure(
            &mut memory,
            "espr 이 뭔지 검색해봐",
            "모델에 전달할 웹 근거 상한에 도달했습니다.",
        )
        .unwrap();

        let restored = load().unwrap();
        assert_eq!(restored.turns.len(), 2);
        assert_eq!(restored.turns[0].role, TuiConversationRole::User);
        assert_eq!(restored.turns[0].content, "espr 이 뭔지 검색해봐");
        assert_eq!(restored.turns[1].role, TuiConversationRole::Error);
        assert!(restored.turns[1].content.contains("웹 근거 상한"));
        assert_eq!(
            conversation_records()
                .last()
                .map(|record| record.kind.as_str()),
            Some("evidence")
        );
    });
}

#[test]
fn web_grounding_is_bounded_and_restored_for_followups_after_resume() {
    with_memory_fixture("web-grounding-resume", || {
        let mut memory = load().unwrap();
        let grounding = vec![WebGroundingEvidence {
            source_id: "source-espr".to_string(),
            title: "Ecodesign for Sustainable Products Regulation".to_string(),
            url: "https://example.com/espr".to_string(),
            excerpt:
                "ESPR means Ecodesign for Sustainable Products Regulation and establishes a framework."
                    .repeat(200),
        }];

        record_exchange(
            &mut memory,
            "ESPR이 뭔지 검색해줘",
            "ESPR 설명 [source-espr]",
            &grounding,
        )
        .unwrap();

        let restored = load().unwrap();
        assert_eq!(restored.web_grounding().len(), 1);
        assert_eq!(restored.web_grounding()[0].source_id, "source-espr");
        assert!(restored.web_grounding()[0]
            .excerpt
            .contains("Ecodesign for Sustainable Products Regulation"));
        assert!(
            restored.web_grounding()[0].excerpt.chars().count()
                <= event_codec::MAX_WEB_GROUNDING_EXCERPT_CHARS
        );
        assert!(conversation_records().iter().any(|record| {
            matches!(
                event_codec::parse_conversation_event(&record.content),
                Some(event_codec::ConversationEvent::WebGrounding(_))
            )
        }));
    });
}

#[test]
fn versioned_events_preserve_legacy_reset_and_runtime_error_compatibility() {
    with_memory_fixture("versioned-event-compatibility", || {
        let identity = ledger::validated_current_identity().unwrap();
        let owner = transcript_owner(&identity);
        transcript::record_session_turn(
            &owner,
            "user",
            "legacy-user",
            "legacy failed request",
            &[],
        )
        .unwrap();
        transcript::record_session_turn(
            &owner,
            "evidence",
            "legacy-error",
            "legacy runtime error",
            &[],
        )
        .unwrap();

        let mut memory = load().unwrap();
        assert_eq!(memory.turns[1].role, TuiConversationRole::Error);
        assert_eq!(memory.turns[1].content, "legacy runtime error");

        clear(&mut memory).unwrap();
        assert!(load().unwrap().turns.is_empty());
        assert!(conversation_records().iter().any(|record| {
            matches!(
                event_codec::parse_conversation_event(&record.content),
                Some(event_codec::ConversationEvent::Reset)
            )
        }));
    });
}
