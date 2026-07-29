use super::*;

#[test]
fn controller_starts_fresh_until_a_session_is_explicitly_resumed() {
    let mut terminal = ScriptedTerminal::new(["/quit"]);
    let mut runtime = ConversationRuntime {
        history: vec![
            TuiConversationTurn {
                role: TuiConversationRole::User,
                content: "이전 질문".to_string(),
            },
            TuiConversationTurn {
                role: TuiConversationRole::Assistant,
                content: "이전 답변".to_string(),
            },
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.reconcile_backend_calls, 1);
    assert!(!terminal.frames[0].contains("이전 질문"));
    assert!(!terminal.frames[0].contains("이전 답변"));
    assert!(terminal.frames[0].contains("새 대화"));
}

#[test]
fn controller_surfaces_status_read_failures_instead_of_silently_zeroing_context() {
    let mut terminal = ScriptedTerminal::new(["/quit"]);
    let mut runtime = ConversationRuntime {
        status_failure: Some("transcript 상태 복원 실패".to_string()),
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("상태 정보를 읽지 못했습니다."));
    assert!(rendered.contains("transcript 상태 복원 실패"));
    assert!(rendered.contains("/doctor"));
}

#[test]
fn resume_command_uses_a_picker_and_rehydrates_only_the_selected_session() {
    let mut terminal = ScriptedTerminal::new(["/resume", "2", "/quit"]);
    let mut runtime = ConversationRuntime {
        history: vec![
            TuiConversationTurn {
                role: TuiConversationRole::User,
                content: "선택한 세션 질문".to_string(),
            },
            TuiConversationTurn {
                role: TuiConversationRole::Assistant,
                content: "선택한 세션 답변".to_string(),
            },
        ],
        session_options: vec![
            session_option("session-current", "현재 세션", true),
            session_option("session-older", "계산기 작업", false),
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.resumed_sessions, ["session-older"]);
    let rendered = terminal.frames.join("\n");
    assert!(rendered.contains("세션 재개"));
    assert!(rendered.contains("계산기 작업"));
    assert!(rendered.contains("선택한 세션 질문"));
    assert!(rendered.contains("선택한 세션 답변"));
}

#[test]
fn typed_scroll_event_pages_history_without_becoming_a_submitted_command() {
    let mut terminal = ScriptedTerminal::new(["1"]);
    terminal.input_events = [
        Ok(crate::runtime_core::terminal::TerminalInputEvent::Submit(
            "/resume".to_string(),
        )),
        Ok(crate::runtime_core::terminal::TerminalInputEvent::Submit(
            "넌 누구야".to_string(),
        )),
        Ok(crate::runtime_core::terminal::TerminalInputEvent::ScrollUp),
        Ok(crate::runtime_core::terminal::TerminalInputEvent::Submit(
            "/quit".to_string(),
        )),
    ]
    .into_iter()
    .collect();
    let history = (1..=24)
        .flat_map(|index| {
            [
                TuiConversationTurn {
                    role: TuiConversationRole::User,
                    content: format!("기록 질문 {index}"),
                },
                TuiConversationTurn {
                    role: TuiConversationRole::Assistant,
                    content: format!("기록 답변 {index}"),
                },
            ]
        })
        .collect();
    let mut runtime = ConversationRuntime {
        history,
        session_options: vec![session_option("session-history", "긴 대화", false)],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.requests, ["넌 누구야"]);
    assert!(
        terminal
            .frames
            .last()
            .expect("scrolled conversation frame")
            .contains("↑ 이전 대화"),
        "typed scroll event should render an older conversation page"
    );
}

#[test]
fn new_command_starts_an_empty_session_instead_of_clearing_old_history() {
    let mut terminal = ScriptedTerminal::new(["/new", "/quit"]);
    let mut runtime = ConversationRuntime {
        history: vec![TuiConversationTurn {
            role: TuiConversationRole::Assistant,
            content: "이전 세션 답변".to_string(),
        }],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.new_session_calls, 1);
    assert_eq!(runtime.clear_history_calls, 0);
    assert!(!terminal.frames.last().unwrap().contains("이전 세션 답변"));
    assert!(terminal.frames.last().unwrap().contains("새 세션"));
}

#[test]
fn clear_command_clears_canonical_and_rendered_conversation() {
    let mut terminal = ScriptedTerminal::new(["/clear", "/quit"]);
    let mut runtime = ConversationRuntime {
        history: vec![
            TuiConversationTurn {
                role: TuiConversationRole::User,
                content: "지울 질문".to_string(),
            },
            TuiConversationTurn {
                role: TuiConversationRole::Assistant,
                content: "지울 답변".to_string(),
            },
        ],
        ..ConversationRuntime::default()
    };

    run_controller(&mut terminal, &mut runtime).unwrap();

    assert_eq!(runtime.clear_history_calls, 1);
    assert!(!terminal.frames.last().unwrap().contains("지울 답변"));
}
