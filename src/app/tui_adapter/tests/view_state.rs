#[test]
fn interactive_view_change_resets_page_and_updates_notice() {
    let mut state = InteractiveState {
        view: InteractiveView::Sessions,
        page: 4,
        selected_id: Some("workflow-selected".to_string()),
        notice: "old notice".to_string(),
        notice_page: 3,
        ..InteractiveState::new()
    };

    state.set_view(InteractiveView::Transcript("session-next".to_string()));

    assert_eq!(
        state.view,
        InteractiveView::Transcript("session-next".to_string())
    );
    assert_eq!(state.page, 0);
    assert_eq!(state.selected_id.as_deref(), Some("workflow-selected"));
    assert_eq!(state.notice, "화면을 변경했습니다.");
    assert_eq!(state.notice_page, 0);
}

#[test]
fn interactive_view_builds_bounded_read_request_from_viewport() {
    let state = InteractiveState {
        view: InteractiveView::ToolOutput("artifact-one".to_string()),
        page: 3,
        ..InteractiveState::new()
    };

    let request = state.read_request(10, 8);

    assert_eq!(
        request,
        TuiReadRequest::ToolOutput {
            artifact_id: "artifact-one".to_string(),
            page: 3,
            budget: TuiReadBudget::bounded(2, 20),
        }
    );
}
