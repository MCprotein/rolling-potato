use super::*;

#[test]
fn launch_contract_enforces_role_tool_and_write_boundaries() {
    let error = validate_launch(
        "explore",
        "task",
        &strings(&["read_file", "render_diff"]),
        &strings(&["src/main.rs"]),
        &strings(&["src/main.rs"]),
        None,
        None,
    )
    .unwrap_err();
    assert!(error.message.contains("role/tool policy"));

    let error = validate_launch(
        "executor",
        "task",
        &strings(&["read_file", "render_diff"]),
        &strings(&["src/main.rs"]),
        &[],
        None,
        None,
    )
    .unwrap_err();
    assert!(error.message.contains("함께 선언"));

    let error = validate_launch(
        "executor",
        "task",
        &strings(&["read_file", "render_diff"]),
        &strings(&["src/main.rs"]),
        &strings(&["README.md"]),
        None,
        None,
    )
    .unwrap_err();
    assert!(error.message.contains("declared read target"));
}

#[test]
fn launch_contract_enforces_exact_task_and_budget_bounds() {
    validate_launch(
        "explore",
        &"x".repeat(MAX_TASK_BYTES),
        &strings(&["read_file"]),
        &strings(&["src/main.rs"]),
        &[],
        Some(MAX_CHAT_TIMEOUT_MS),
        Some(MAX_MAX_TOKENS),
    )
    .unwrap();
    for error in [
        validate_launch(
            "explore",
            &"x".repeat(MAX_TASK_BYTES + 1),
            &strings(&["read_file"]),
            &strings(&["src/main.rs"]),
            &[],
            None,
            None,
        )
        .unwrap_err(),
        validate_launch(
            "explore",
            "task",
            &strings(&["read_file"]),
            &strings(&["src/main.rs"]),
            &[],
            Some(0),
            None,
        )
        .unwrap_err(),
        validate_launch(
            "explore",
            "task",
            &strings(&["read_file"]),
            &strings(&["src/main.rs"]),
            &[],
            None,
            Some(MAX_MAX_TOKENS + 1),
        )
        .unwrap_err(),
    ] {
        assert_eq!(error.code, 2);
    }
}

#[test]
fn launch_contract_rejects_traversal_duplicates_and_excess_paths() {
    for paths in [
        strings(&["../secret"]),
        strings(&["src/main.rs", "src/main.rs"]),
        strings(&["a", "b", "c", "d", "e"]),
        strings(&["C:\\secret"]),
    ] {
        let error = validate_launch(
            "explore",
            "task",
            &strings(&["read_file"]),
            &paths,
            &[],
            None,
            None,
        )
        .unwrap_err();
        assert!(matches!(error.code, 2 | 3));
    }
}

#[test]
fn lifecycle_transition_matrix_is_closed() {
    let terminal = [
        SubagentStatus::Completed,
        SubagentStatus::Blocked,
        SubagentStatus::Failed,
        SubagentStatus::Cancelled,
        SubagentStatus::TimedOut,
    ];
    assert!(SubagentStatus::Requested.permits(SubagentStatus::Admitted));
    assert!(SubagentStatus::Admitted.permits(SubagentStatus::Running));
    for status in terminal {
        assert!(SubagentStatus::Running.permits(status));
        assert!(!status.permits(SubagentStatus::Requested));
    }
    assert!(!SubagentStatus::Requested.permits(SubagentStatus::Running));
    assert!(!SubagentStatus::Admitted.permits(SubagentStatus::Completed));
}
