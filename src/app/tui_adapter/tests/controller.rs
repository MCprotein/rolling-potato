#[test]
fn live_controller_compile_time_boundary_uses_only_runtime_and_terminal_authority() {
    let controller = include_str!("../../../surfaces/tui/controller.rs");
    let command_dispatch = include_str!("../../../surfaces/tui/controller/command_dispatch.rs");
    let workflow_dispatch =
        include_str!("../../../surfaces/tui/controller/command_dispatch/workflow.rs");
    let request_submission =
        include_str!("../../../surfaces/tui/controller/request_submission.rs");
    for forbidden in [
        "use crate::runtime;",
        "crate::runtime::",
        "use crate::approval",
        "use crate::{evidence",
        "ledger::",
        "observability::",
        "crate::patch::",
        "state::",
    ] {
        for live in [
            controller,
            command_dispatch,
            workflow_dispatch,
            request_submission,
        ] {
            assert!(
                !live.contains(forbidden),
                "live boundary escaped via {forbidden}"
            );
        }
    }
    assert!(controller.contains("mod command_dispatch;"));
    assert!(controller.contains("mod request_submission;"));
    assert!(controller.contains("runtime.read_tui_page(request)"));
    assert!(command_dispatch.contains("workflow::dispatch_workflow"));
    assert!(command_dispatch.contains("fn dispatch_line"));
    assert!(workflow_dispatch.contains("runtime.dispatch_tui_intent"));
    assert!(workflow_dispatch.contains("fn dispatch_workflow"));
    assert!(controller.contains("trait TuiRuntimePort"));
    assert!(request_submission.contains("runtime.submit_request"));
}

#[test]
fn one_shot_approval_and_diff_views_use_the_canonical_runtime_facade() {
    let composition = include_str!("../report_composition.rs");

    assert!(composition.contains("canonical_read_page(TuiReadRequest::Approvals"));
    assert!(composition.contains("canonical_read_page(TuiReadRequest::Diff"));
    assert!(!composition.contains("proposal_summaries("));
    assert!(!composition.contains("request_summaries("));
    assert!(!composition.contains("proposal_detail("));
}

#[test]
fn interactive_controller_exits_cleanly_and_never_emits_terminal_injection() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-interactive-controller-test");
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var("RPOTATO_TEST_SKIP_UPDATE_CHECK", "1");
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new(["/model", "", "/help", "/compact", "/quit"]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter::default()).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_TEST_SKIP_UPDATE_CHECK");
    let _ = std::fs::remove_dir_all(root);
    assert!(terminal.frames.len() >= 2);
    assert!(terminal
        .frames
        .iter()
        .all(|frame| !frame.contains('\u{001b}')));
    assert!(terminal.frames.iter().any(|frame| frame.contains("›")));
    assert!(
        terminal
            .frames
            .iter()
            .any(|frame| frame.contains("gemma-4-e4b")),
        "{:#?}",
        terminal.frames
    );
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("context 131k")));
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("16 GB 적합성은 미확정")));
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("/compact: 현재 대화 컨텍스트 압축")));
    assert!(terminal
        .frames
        .iter()
        .any(|frame| frame.contains("기본 모델이 선택되지 않았습니다")));
}

#[test]
fn interactive_controller_notifies_and_applies_update_without_leaving_tui() {
    let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
    let root = test_root("rpotato-interactive-update-test");
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
    std::env::set_var(
        "RPOTATO_TEST_LATEST_RELEASE_JSON",
        r#"{"tag_name":"v9.0.0","assets":[{"name":"rpotato-v9.0.0-checksums.txt","state":"uploaded"}]}"#,
    );
    std::env::set_var(
        "RPOTATO_TEST_UPDATE_REPORT",
        "rpotato update\n- status: updated\n- installed: v9.0.0",
    );
    std::fs::create_dir_all(root.join("project")).unwrap();
    crate::app::workflow_adapter::state::initialize().unwrap();
    let mut terminal = ScriptedTerminal::new(["/update", "2", "/quit"]);

    run_controller(&mut terminal, &mut TuiRuntimeAdapter::default()).unwrap();

    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    std::env::remove_var("RPOTATO_TEST_LATEST_RELEASE_JSON");
    std::env::remove_var("RPOTATO_TEST_UPDATE_REPORT");
    let _ = std::fs::remove_dir_all(root);
    let rendered = terminal.frames.join("\n");
    assert!(
        !terminal.frames[0].contains("새 rpotato 버전이 있습니다"),
        "the first frame must render before the network-backed update check"
    );
    assert!(terminal.frames[1].contains("새 rpotato 버전이 있습니다"));
    assert!(rendered.contains("새 rpotato 버전이 있습니다"));
    assert!(rendered.contains("/update 를 입력하면"));
    assert!(rendered.contains("업데이트 확인"));
    assert!(rendered.contains("1. 취소"));
    assert!(rendered.contains("2. 업데이트 시작"));
    assert!(!rendered.contains("yes를 입력"));
    assert!(rendered.contains("SHA-256 검증"));
    assert!(rendered.contains("status: updated"));
    assert!(rendered.contains("installed: v9.0.0"));
}
