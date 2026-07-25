use super::*;

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
        "내 이름은 감자야",
        "이 세션에서만 기억하겠습니다.",
    )
    .unwrap();

    crate::app::workflow_adapter::state::session_new_report().unwrap();
    let mut second_memory = crate::app::tui_adapter::session_memory::load().unwrap();
    crate::app::tui_adapter::session_memory::record_exchange(
        &mut second_memory,
        "두 번째 세션 질문",
        "두 번째 세션 답변",
    )
    .unwrap();

    let mut runtime = TuiRuntimeAdapter::default();
    let options = runtime.session_options().unwrap();
    assert!(options
        .iter()
        .any(|option| option.session_id == first_session.session_id));

    let transition = runtime.resume_session(&first_session.session_id).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = std::fs::remove_dir_all(root);
    assert_eq!(transition.session_id, first_session.session_id);
    assert_eq!(
        transition
            .turns
            .iter()
            .map(|turn| turn.content.as_str())
            .collect::<Vec<_>>(),
        ["내 이름은 감자야", "이 세션에서만 기억하겠습니다."]
    );
    assert!(!transition
        .turns
        .iter()
        .any(|turn| turn.content.contains("두 번째 세션")));
}

fn test_root(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
}
