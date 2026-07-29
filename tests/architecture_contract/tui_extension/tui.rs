#[test]
fn v03713_storage_compatibility_is_a_pure_codec_boundary() {
    for owner in [
        "src/runtime_core/workflow/storage_compat/ledger.rs",
        "src/runtime_core/workflow/storage_compat/transcript.rs",
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for forbidden in [
            "use std::fs",
            "OpenOptions",
            "std::io::Write",
            ".exists()",
            "fs::read",
            "fs::create_dir",
        ] {
            assert!(
                !source.contains(forbidden),
                "storage compatibility codec owns filesystem behavior: {owner} -> {forbidden}"
            );
        }
    }
}

#[test]
fn v0471_tui_controller_support_responsibilities_are_split() {
    let controller = fs::read_to_string("src/surfaces/tui/controller.rs").unwrap();
    for (module, owner, marker) in [
        (
            "attachments",
            "src/surfaces/tui/controller/attachments.rs",
            "fn looks_like_attachment_path",
        ),
        (
            "model_selection",
            "src/surfaces/tui/controller/model_selection.rs",
            "fn choose_model",
        ),
        (
            "session_selection",
            "src/surfaces/tui/controller/session_selection.rs",
            "fn resume_session",
        ),
        (
            "terminal_flow",
            "src/surfaces/tui/controller/terminal_flow.rs",
            "fn terminal_fault_error",
        ),
    ] {
        assert!(
            controller
                .lines()
                .any(|line| line == format!("mod {module};")),
            "TUI controller does not register support owner: {module}"
        );
        let source = fs::read_to_string(owner).unwrap();
        assert!(
            source.contains(marker),
            "TUI controller support owner is missing responsibility: {owner} -> {marker}"
        );
    }
    assert!(
        controller.lines().count() < 500,
        "TUI controller regrew beyond command-loop ownership"
    );
}
#[test]
fn tui_conversation_journeys_have_bounded_feature_owners() {
    let root = fs::read_to_string("src/app/tui_adapter/conversation_tests.rs").unwrap();
    let owners = [
        ("attachment_layout", 175),
        ("progress_model", 175),
        ("rendering", 175),
        ("session", 200),
        ("web", 175),
    ];

    assert!(root.lines().count() < 300);
    for (owner, line_budget) in owners {
        let relative = format!("conversation_tests/{owner}.rs");
        assert!(
            root.contains(&relative),
            "conversation journey root does not register {owner}"
        );
        let source = fs::read_to_string(format!("src/app/tui_adapter/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "conversation journey {owner} exceeded its {line_budget}-line budget"
        );
    }
}

#[test]
fn tui_attachment_capture_and_composition_have_bounded_owners() {
    let root = fs::read_to_string("src/app/tui_adapter/attachment.rs").unwrap();
    let owners = [
        ("capture", 200, "fn capture("),
        ("compose", 250, "fn compose_request("),
        ("format", 125, "fn attachment_kind("),
        ("path", 75, "fn normalized_source_path("),
        ("tests", 350, "captures_text_into_app_data"),
    ];

    assert!(root.lines().count() < 75);
    for (owner, line_budget, marker) in owners {
        let relative = format!("attachment/{owner}.rs");
        assert!(
            root.contains(&relative) || root.contains(&format!("mod {owner};")),
            "attachment facade does not register {owner}"
        );
        let source = fs::read_to_string(format!("src/app/tui_adapter/{relative}")).unwrap();
        assert!(
            source.contains(marker),
            "attachment owner {owner} is missing {marker}"
        );
        assert!(
            source.lines().count() < line_budget,
            "attachment owner {owner} exceeded its {line_budget}-line budget"
        );
    }
}

#[test]
fn terminal_live_input_has_a_semantic_keymap_owner() {
    let input = fs::read_to_string("src/adapters/terminal/native/live_input.rs").unwrap();
    let keymap = fs::read_to_string("src/adapters/terminal/native/live_input/keymap.rs").unwrap();

    assert!(input.lines().any(|line| line == "mod keymap;"));
    assert!(keymap.contains("pub(super) enum Action"));
    assert!(keymap.contains("pub(super) fn decode_escape"));
    assert!(!input.contains("enum Action"));
    assert!(input.lines().count() < 250);
    assert!(keymap.lines().count() < 100);
}

#[test]
fn tui_render_responsibilities_have_bounded_owners() {
    let render = fs::read_to_string("src/surfaces/tui/render.rs").unwrap();
    for (module, owner, marker, line_budget) in [
        (
            "chrome",
            "src/surfaces/tui/render/chrome.rs",
            "fn render_composer",
            200,
        ),
        (
            "conversation",
            "src/surfaces/tui/render/conversation.rs",
            "fn render_frame",
            225,
        ),
        (
            "notice",
            "src/surfaces/tui/render/notice.rs",
            "fn render_lines",
            75,
        ),
        (
            "report_layout",
            "src/surfaces/tui/render/report_layout.rs",
            "fn push_wrapped",
            175,
        ),
        (
            "text",
            "src/surfaces/tui/render/text.rs",
            "fn sanitize_terminal_text",
            175,
        ),
    ] {
        assert!(
            render.lines().any(|line| line == format!("mod {module};")),
            "TUI render does not register responsibility owner: {module}"
        );
        let source = fs::read_to_string(owner).unwrap();
        assert!(
            source.contains(marker),
            "TUI render responsibility owner is missing: {owner} -> {marker}"
        );
        assert!(
            source.lines().count() < line_budget,
            "TUI render responsibility owner exceeded its {line_budget}-line budget: {owner}"
        );
    }
    assert!(
        render.lines().count() < 150,
        "TUI render facade regrew beyond frame routing ownership"
    );
}

#[test]
fn v0471_patch_intent_classification_and_model_action_are_split() {
    let intent = fs::read_to_string("src/runtime_core/patch/intent.rs").unwrap();
    for (module, owner, marker) in [
        (
            "classification",
            "src/runtime_core/patch/intent/classification.rs",
            "fn classify",
        ),
        (
            "model_action",
            "src/runtime_core/patch/intent/model_action.rs",
            "fn parse_model_action",
        ),
    ] {
        assert!(
            intent.lines().any(|line| line == format!("mod {module};")),
            "patch intent does not register responsibility owner: {module}"
        );
        let source = fs::read_to_string(owner).unwrap();
        assert!(
            source.contains(marker),
            "patch intent responsibility owner is missing: {owner} -> {marker}"
        );
    }
    assert!(
        intent.lines().count() < 300,
        "patch intent facade regrew beyond contracts and action planning"
    );
}
