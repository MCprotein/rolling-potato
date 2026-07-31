use super::*;

#[test]
fn natural_requests_use_agent_progress_until_the_model_selects_a_tool() {
    let request = "2026년 월드컵 결과 검색해서 알려줘";
    let mut terminal = ScriptedTerminal::new([request, "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.requests, [request]);
    assert!(terminal.frames[1].contains("처리 중"));
    assert!(terminal.frames[1].contains("에이전트가 요청을 처리하고 있습니다…"));
}

#[test]
fn slow_requests_refresh_a_spinner_and_live_context_estimate() {
    let mut terminal = ScriptedTerminal::new(["느린 요청", "/quit"]);
    let mut runtime = ConversationRuntime {
        submit_delay_ms: 280,
        context_estimate: Some(321),
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert!(
        terminal
            .frames
            .iter()
            .filter(|frame| frame.contains("처리 중"))
            .count()
            >= 2
    );
    assert!(rendered.contains("ctx ~321/"));
    assert!(rendered.contains("경과"));
}

#[test]
fn progress_frame_failure_after_dispatch_never_invites_request_replay() {
    let mut terminal = ScriptedTerminal::new(["느린 요청", "/quit"]);
    terminal.frame_fault_at = Some((
        crate::runtime_core::terminal::FrameWriteBoundary::PostDispatch,
        crate::runtime_core::terminal::TerminalFault::FrameWrite,
    ));
    let mut runtime = ConversationRuntime {
        submit_delay_ms: 280,
        ..ConversationRuntime::default()
    };

    let error = run_controller(&mut terminal, &mut runtime).unwrap_err();

    assert_eq!(runtime.requests, ["느린 요청"]);
    assert!(error.message.contains("terminal.frame-write.post-dispatch"));
    assert!(error.message.contains("요청을 다시 보내지 않습니다"));
    assert!(!error.message.contains("런타임 요청을 보내지 않았습니다"));
}

#[test]
fn model_command_uses_keyboard_choices_and_applies_the_selection() {
    let mut terminal = ScriptedTerminal::new(["/model", "2", "2", "/quit"]);
    let mut runtime = ConversationRuntime {
        model_options: vec![
            model_option("small", "Small", true, false),
            model_option("recommended", "Recommended", false, true),
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.setup_models, ["recommended"]);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("모델 선택"));
    assert!(rendered.contains("Recommended"));
    assert!(rendered.contains("모델 변경 확인"));
    assert!(rendered.contains("모델 적용 완료: recommended"));
}

#[test]
fn model_confirmation_defaults_to_cancel_without_applying_the_selection() {
    let mut terminal = ScriptedTerminal::new(["/model", "2", "1", "/quit"]);
    let mut runtime = ConversationRuntime {
        model_options: vec![
            model_option("small", "Small", true, false),
            model_option("recommended", "Recommended", false, true),
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert!(runtime.setup_models.is_empty());
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("모델 변경 확인"));
    assert!(rendered.contains("1. 취소"));
    assert!(rendered.contains("2. 다운로드하고 적용"));
    assert!(rendered.contains("모델 변경을 취소했습니다."));
}

#[test]
fn cached_model_switch_is_labeled_as_reuse_instead_of_a_new_download() {
    let mut cached = model_option("cached", "Cached", false, true);
    cached.model_cached = true;
    cached.vision_projector_cached = true;
    let mut terminal = ScriptedTerminal::new(["/model", "2", "2", "/quit"]);
    let mut runtime = ConversationRuntime {
        model_options: vec![model_option("current", "Current", true, false), cached],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.setup_models, ["cached"]);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("local cache"));
    assert!(rendered.contains("기존 모델로 평가 전환"));
    assert!(rendered.contains("기존 모델 cache/SHA-256 검증"));
    assert!(!rendered.contains("Cached · download"));
}

#[test]
fn update_confirmation_defaults_to_cancel_without_calling_the_updater() {
    let mut terminal = ScriptedTerminal::new(["/update", "1", "/quit"]);
    let mut runtime = ConversationRuntime::default();

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.update_calls, 0);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("업데이트 확인"));
    assert!(rendered.contains("1. 취소"));
    assert!(rendered.contains("2. 업데이트 시작"));
    assert!(rendered.contains("업데이트를 취소했습니다."));
    assert!(!rendered.contains("yes를 입력"));
}
