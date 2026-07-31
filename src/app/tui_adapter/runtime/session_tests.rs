use super::*;
use crate::surfaces::tui::controller::TuiRuntimePort;

#[test]
fn fresh_session_compaction_never_targets_the_previous_session() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-tui-fresh-compaction-boundary-test");
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    crate::app::workflow_adapter::state::initialize().unwrap();

    let previous = crate::app::workflow_adapter::ledger::validated_current_identity().unwrap();
    let mut memory = crate::app::tui_adapter::session_memory::load().unwrap();
    crate::app::tui_adapter::session_memory::record_exchange(
        &mut memory,
        "이전 세션 질문",
        "이전 세션 답변",
        &[],
    )
    .unwrap();

    let mut runtime = TuiRuntimeAdapter::default();
    let _ = runtime.compact_context();
    let current = crate::app::workflow_adapter::ledger::validated_current_identity().unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(root);
    assert_ne!(
        current.session_id, previous.session_id,
        "fresh 화면의 /compact가 이전 durable session을 대상으로 삼았습니다."
    );
}

#[test]
fn explicit_session_resume_restores_only_the_selected_conversation() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-tui-explicit-session-resume-test");
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::env::set_var("RPOTATO_PROJECT_ROOT", &project);
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    crate::app::workflow_adapter::state::initialize().unwrap();

    let first_session = crate::app::workflow_adapter::ledger::validated_current_identity().unwrap();
    let mut first_memory = crate::app::tui_adapter::session_memory::load().unwrap();
    crate::app::tui_adapter::session_memory::record_exchange(
        &mut first_memory,
        "ESPR이 뭔지 검색해줘",
        "ESPR 설명 [source-espr]",
        &[crate::app::web_search_adapter::WebGroundingEvidence {
            source_id: "source-espr".to_string(),
            title: "Ecodesign for Sustainable Products Regulation".to_string(),
            url: "https://example.com/espr".to_string(),
            excerpt: "ESPR is the Ecodesign for Sustainable Products Regulation.".to_string(),
        }],
    )
    .unwrap();

    crate::app::workflow_adapter::state::session_new_report().unwrap();
    let mut second_memory = crate::app::tui_adapter::session_memory::load().unwrap();
    crate::app::tui_adapter::session_memory::record_exchange(
        &mut second_memory,
        "두 번째 세션 질문",
        "두 번째 세션 답변",
        &[],
    )
    .unwrap();

    let mut runtime = TuiRuntimeAdapter::default();
    let options = runtime.session_options().unwrap();
    assert!(options
        .iter()
        .any(|option| option.session_id == first_session.session_id));
    runtime
        .web_pages
        .record(crate::adapters::web_search::WebPageEvidence {
            source_id: "source-session-page".to_string(),
            requested_url: "https://example.com/previous".to_string(),
            final_url: "https://example.com/previous".to_string(),
            title: Some("Previous page".to_string()),
            content: "must not cross session resume".to_string(),
        });

    let transition = runtime.resume_session(&first_session.session_id).unwrap();

    assert_eq!(transition.session_id, first_session.session_id);
    assert_eq!(
        transition
            .turns
            .iter()
            .map(|turn| turn.content.as_str())
            .collect::<Vec<_>>(),
        ["ESPR이 뭔지 검색해줘", "ESPR 설명 [source-espr]"]
    );
    assert!(!transition
        .turns
        .iter()
        .any(|turn| turn.content.contains("두 번째 세션")));
    assert_eq!(runtime.web_pages.len(), 0);
    assert_eq!(
        runtime.conversation_memory().unwrap().web_grounding()[0].source_id,
        "source-espr"
    );
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn web_source_options_are_recent_first_and_select_the_find_target() {
    let mut runtime = TuiRuntimeAdapter::default();
    for (id, title) in [("source-one", "One"), ("source-two", "Two")] {
        runtime
            .web_pages
            .record(crate::adapters::web_search::WebPageEvidence {
                source_id: id.to_string(),
                requested_url: format!("https://example.com/{id}"),
                final_url: format!("https://example.com/{id}"),
                title: Some(title.to_string()),
                content: title.to_string(),
            });
    }

    let options = runtime.web_source_options();
    assert_eq!(
        options
            .iter()
            .map(|option| option.source_id.as_str())
            .collect::<Vec<_>>(),
        ["source-two", "source-one"]
    );
    assert!(options[0].current);
    assert!(runtime.select_web_source("source-one").is_ok());
    assert_eq!(runtime.web_pages.current().unwrap().source_id, "source-one");
}

fn test_root(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
}
