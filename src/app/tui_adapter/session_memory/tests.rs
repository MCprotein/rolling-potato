use std::collections::BTreeSet;

use super::*;

fn with_memory_fixture(test_name: &str, test: impl FnOnce()) {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!(
        "rpotato-tui-session-memory-{test_name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    crate::app::workflow_adapter::state::initialize().unwrap();

    test();

    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    let _ = std::fs::remove_dir_all(root);
}

fn conversation_records(
) -> Vec<crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord> {
    let identity = ledger::validated_current_identity().unwrap();
    transcript::records_for_session(&identity.session_id)
        .unwrap()
        .into_iter()
        .filter(|record| record.workflow_id == CONVERSATION_STREAM_ID)
        .collect()
}

#[test]
fn canonical_memory_restores_only_complete_pairs_and_honors_reset_boundaries() {
    with_memory_fixture("complete-pairs", || {
        let mut memory = load().unwrap();
        assert!(memory.turns.is_empty());

        record_exchange(&mut memory, "내 이름은 감자야", "알겠습니다.").unwrap();
        assert_eq!(memory.turns.len(), 2);
        assert_eq!(load().unwrap(), memory);

        clear(&mut memory).unwrap();
        assert!(memory.turns.is_empty());
        assert!(load().unwrap().turns.is_empty());
    });
}

#[test]
fn reset_is_a_unique_causal_head_for_repeated_questions() {
    with_memory_fixture("reset-causal-head", || {
        let mut memory = load().unwrap();
        record_exchange(&mut memory, "안녕", "첫 번째 답변").unwrap();
        clear(&mut memory).unwrap();
        record_exchange(&mut memory, "안녕", "두 번째 답변").unwrap();
        clear(&mut memory).unwrap();
        clear(&mut memory).unwrap();

        let records = conversation_records();
        let ids = records
            .iter()
            .map(|record| record.record_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            records.len(),
            ids.len(),
            "모든 append record id는 고유해야 합니다."
        );
        assert_eq!(
            records
                .iter()
                .filter(|record| record.content == RESET_MARKER)
                .count(),
            3
        );
        assert!(load().unwrap().turns.is_empty());
    });
}

#[test]
fn reset_discards_an_orphan_user_before_a_later_model_record() {
    with_memory_fixture("reset-orphan", || {
        let identity = ledger::validated_current_identity().unwrap();
        let owner = transcript_owner(&identity);
        transcript::record_session_turn(
            &owner,
            "user",
            "conversation-orphan-user",
            "응답 없는 요청",
            &[],
        )
        .unwrap();
        let mut memory = load().unwrap();
        assert!(memory.turns.is_empty());

        clear(&mut memory).unwrap();
        transcript::record_session_turn(
            &owner,
            "model",
            "conversation-after-reset-model",
            "잘못 결합되면 안 되는 답변",
            &[],
        )
        .unwrap();

        assert!(
            load().unwrap().turns.is_empty(),
            "reset 이전 orphan user와 이후 model은 한 pair가 아니어야 합니다."
        );
    });
}

#[test]
fn coding_exchange_is_canonical_and_prompt_history_is_bounded() {
    with_memory_fixture("coding-and-bounded", || {
        let mut memory = load().unwrap();
        record_exchange(
            &mut memory,
            "src/lib.rs를 리팩토링해줘",
            "변경 제안을 준비했습니다.",
        )
        .unwrap();
        assert_eq!(load().unwrap().turns, memory.turns);

        memory.turns = (0..600)
            .map(|index| TuiConversationTurn {
                role: if index % 2 == 0 {
                    TuiConversationRole::User
                } else {
                    TuiConversationRole::Assistant
                },
                content: format!("turn-{index}"),
            })
            .collect();
        let prompt = memory.prompt_history();
        assert_eq!(prompt.len(), MAX_PROMPT_HISTORY_TURNS);
        assert_eq!(prompt.first().unwrap().content, "turn-88");
        assert_eq!(prompt.last().unwrap().content, "turn-599");
    });
}
