#[test]
fn restricted_browser_process_and_protocol_have_separate_bounded_owners() {
    let adapters = fs::read_to_string("src/adapters/mod.rs").unwrap();
    let runtime = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    let facade = fs::read_to_string("src/adapters/browser.rs").unwrap();
    let actions = fs::read_to_string("src/adapters/browser/actions.rs").unwrap();
    let accessibility =
        fs::read_to_string("src/adapters/browser/actions/accessibility.rs").unwrap();
    let protocol_values =
        fs::read_to_string("src/adapters/browser/actions/protocol_values.rs").unwrap();
    let action_tests = fs::read_to_string("src/adapters/browser/actions/tests.rs").unwrap();
    let discovery = fs::read_to_string("src/adapters/browser/discovery.rs").unwrap();
    let session = fs::read_to_string("src/adapters/browser/session.rs").unwrap();
    let protocol = fs::read_to_string("src/adapters/browser/protocol.rs").unwrap();
    let proxy = fs::read_to_string("src/adapters/browser/proxy.rs").unwrap();
    let websocket = fs::read_to_string("src/adapters/browser/websocket.rs").unwrap();
    let tests = fs::read_to_string("src/adapters/browser/tests.rs").unwrap();
    let browser_policy = fs::read_to_string("src/runtime_core/browser.rs").unwrap();
    let interaction = fs::read_to_string("src/runtime_core/browser/interaction.rs").unwrap();
    let interaction_tests = fs::read_to_string("src/runtime_core/browser/tests.rs").unwrap();
    let browser_app = fs::read_to_string("src/app/browser_adapter.rs").unwrap();
    let browser_routing = fs::read_to_string("src/app/browser_adapter/routing.rs").unwrap();
    let browser_search_form = fs::read_to_string("src/app/browser_adapter/search_form.rs").unwrap();
    let browser_app_tests = fs::read_to_string("src/app/browser_adapter/tests.rs").unwrap();
    let conversation = fs::read_to_string("src/app/tui_adapter/conversation.rs").unwrap();
    let conversation_decision =
        fs::read_to_string("src/app/tui_adapter/conversation/decision.rs").unwrap();
    let conversation_local_facts =
        fs::read_to_string("src/app/tui_adapter/conversation/local_facts.rs").unwrap();
    let conversation_presentation =
        fs::read_to_string("src/app/tui_adapter/conversation/presentation.rs").unwrap();
    let conversation_reply =
        fs::read_to_string("src/app/tui_adapter/conversation/reply.rs").unwrap();
    let conversation_reply_prompt =
        fs::read_to_string("src/app/tui_adapter/conversation/reply/prompt.rs").unwrap();
    let conversation_tests =
        fs::read_to_string("src/app/tui_adapter/conversation/tests/mod.rs").unwrap();
    let conversation_decision_tests =
        fs::read_to_string("src/app/tui_adapter/conversation/tests/decision.rs").unwrap();
    let tui_request = fs::read_to_string("src/app/tui_adapter/runtime/request.rs").unwrap();
    let tui_request_routing =
        fs::read_to_string("src/app/tui_adapter/runtime/request/routing.rs").unwrap();

    assert!(adapters.contains("pub(crate) mod browser;"));
    assert!(runtime.contains("pub(crate) mod browser;"));
    for owner in [
        "actions",
        "discovery",
        "protocol",
        "proxy",
        "session",
        "websocket",
    ] {
        assert!(
            facade.lines().any(|line| line == format!("mod {owner};")),
            "browser adapter facade does not register {owner}"
        );
    }
    for path in [
        "src/adapters/browser/actions.rs",
        "src/adapters/browser/actions/accessibility.rs",
        "src/adapters/browser/actions/protocol_values.rs",
        "src/adapters/browser/actions/tests.rs",
        "src/adapters/browser/discovery.rs",
        "src/adapters/browser/protocol.rs",
        "src/adapters/browser/proxy.rs",
        "src/adapters/browser/session.rs",
        "src/adapters/browser/tests.rs",
        "src/adapters/browser/websocket.rs",
        "src/app/browser_adapter.rs",
        "src/app/browser_adapter/routing.rs",
        "src/app/browser_adapter/search_form.rs",
        "src/app/browser_adapter/tests.rs",
        "src/app/tui_adapter/conversation/decision.rs",
        "src/app/tui_adapter/conversation/local_facts.rs",
        "src/app/tui_adapter/conversation/presentation.rs",
        "src/app/tui_adapter/conversation/reply.rs",
        "src/app/tui_adapter/conversation/reply/prompt.rs",
        "src/app/tui_adapter/conversation/tests/decision.rs",
        "src/app/tui_adapter/conversation/tests/local_facts.rs",
        "src/app/tui_adapter/conversation/tests/mod.rs",
        "src/app/tui_adapter/conversation/tests/presentation.rs",
        "src/app/tui_adapter/conversation/tests/reply.rs",
        "src/runtime_core/browser.rs",
        "src/runtime_core/browser/interaction.rs",
        "src/runtime_core/browser/tests.rs",
    ] {
        assert!(Path::new(path).is_file(), "missing browser owner: {path}");
    }

    assert!(discovery.contains("Google Chrome.app/Contents/MacOS/Google Chrome"));
    assert!(discovery.contains("Google/Chrome/Application/chrome.exe"));
    assert!(discovery.contains("google-chrome-stable"));
    assert!(session.contains("--remote-debugging-port=0"));
    assert!(session.contains("--user-data-dir="));
    assert!(session.contains("--proxy-server=http://"));
    assert!(session.contains("--proxy-bypass-list=<-loopback>"));
    assert!(session.contains("--disable-quic"));
    assert!(session.contains("--force-webrtc-ip-handling-policy=disable_non_proxied_udp"));
    assert!(session.contains("browser_command_forces_all_page_traffic_through_the_public_proxy"));
    assert!(session.contains("terminate_child_tree"));
    assert!(proxy.contains("resolve_public_browser_target"));
    assert!(proxy.contains("CONNECT"));
    assert!(proxy.contains("MAX_ACTIVE_TUNNELS"));
    assert!(!proxy.contains("direct://"));
    assert!(protocol.contains("/devtools/browser/"));
    assert!(protocol.contains("enum CdpMethod"));
    assert!(!protocol.contains("Runtime.evaluate"));
    assert!(!protocol.contains("pub(crate) enum CdpMethod"));
    assert!(actions.contains("BrowserInteractionSession"));
    assert!(actions.contains("AccessibilityGetFullAxTree"));
    assert!(actions.contains("DomGetBoxModel"));
    for forbidden in ["Runtime.evaluate", "querySelector", "xpath", "XPath"] {
        assert!(
            !actions.contains(forbidden),
            "browser action owner must not expose {forbidden}"
        );
    }
    assert!(accessibility.contains("ObservedTargetSeed"));
    assert!(protocol_values.contains("fn box_center"));
    assert!(action_tests.contains("forbidden_targets_and_url_schemes_never_reach_the_protocol"));
    assert!(browser_policy.contains("pub(crate) struct ElementHandle"));
    assert!(interaction.contains("pub(crate) struct BrowserActionBudget"));
    assert!(interaction.contains("max_interactions"));
    assert!(interaction_tests.contains("observation_issues_opaque_handles"));
    assert!(websocket.contains("MAX_FRAME_BYTES"));
    assert!(websocket.contains("is_loopback()"));
    assert!(tests.contains("isolated_browser_session_cleans_up_process_group_and_profile"));
    assert!(tests.contains("scopes_target_commands_to_an_attached_session"));
    assert!(tests.contains("oversized_protocol_frame_is_rejected_before_allocating_its_payload"));
    assert!(browser_app.contains("mod routing;"));
    assert!(browser_app.contains("mod search_form;"));
    assert!(browser_routing.contains("fn deterministic_browser_fallback("));
    assert!(browser_routing.contains("BrowserSearchRequest"));
    assert!(!browser_routing.contains("BROWSER TOOL:"));
    assert!(browser_search_form.contains("BrowserControl"));
    assert!(!browser_search_form.contains("querySelector"));
    assert!(!browser_search_form.contains("Runtime.evaluate"));
    assert!(browser_app_tests.contains("generic_search_form_e2e_uses_opaque_handles"));
    assert!(browser_app_tests.contains("delayed_initial_page_readiness_is_polled_before_typing"));
    assert!(browser_app_tests.contains("delayed_result_page_readiness_is_polled_before_reporting"));
    assert!(browser_app_tests.contains("private_redirect_result_is_rejected"));
    for owner in ["decision", "local_facts", "presentation", "reply"] {
        assert!(
            conversation
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "conversation facade does not register {owner}"
        );
        assert!(
            conversation_tests
                .lines()
                .any(|line| line == format!("mod {owner};")),
            "conversation test facade does not register {owner}"
        );
    }
    assert!(conversation.lines().any(|line| line == "mod tests;"));
    assert!(
        conversation_decision_tests.contains("history_only_secret_cannot_become_network_tool_input")
    );
    assert!(conversation_decision.contains("RequestDecision::BrowserTool"));
    assert!(conversation_decision.contains("generate_structured_candidate_for_user"));
    assert!(conversation_decision.contains("TURN_DECISION_JSON_SCHEMA"));
    assert!(conversation_decision.contains("deterministic_browser_fallback"));
    assert!(tui_request.lines().any(|line| line == "mod routing;"));
    assert!(tui_request_routing.contains("RequestDecision::BrowserTool"));
    assert!(conversation_reply.lines().any(|line| line == "mod prompt;"));
    assert!(conversation_reply_prompt.contains("fn assemble_plain_prompt("));
    assert!(conversation_reply_prompt.contains("fn assemble_vision_prompt("));

    assert!(facade.lines().count() < 25);
    assert!(actions.lines().count() < 350);
    assert!(accessibility.lines().count() < 225);
    assert!(protocol_values.lines().count() < 150);
    assert!(action_tests.lines().count() < 300);
    assert!(discovery.lines().count() < 300);
    assert!(session.lines().count() < 350);
    assert!(protocol.lines().count() < 275);
    assert!(proxy.lines().count() < 375);
    assert!(websocket.lines().count() < 425);
    assert!(tests.lines().count() < 375);
    assert!(browser_app.lines().count() < 75);
    assert!(browser_routing.lines().count() < 225);
    assert!(browser_search_form.lines().count() < 250);
    assert!(browser_app_tests.lines().count() < 400);
    assert!(browser_policy.lines().count() < 225);
    assert!(interaction.lines().count() < 350);
    assert!(interaction_tests.lines().count() < 225);
    assert!(conversation.lines().count() < 50);
    assert!(conversation_decision.lines().count() < 300);
    assert!(conversation_local_facts.lines().count() < 300);
    assert!(conversation_presentation.lines().count() < 125);
    assert!(conversation_reply.lines().count() < 125);
    assert!(conversation_reply_prompt.lines().count() < 100);
    assert!(conversation_tests.lines().count() < 20);
    for (path, line_budget) in [
        (
            "src/app/tui_adapter/conversation/tests/decision.rs",
            250,
        ),
        (
            "src/app/tui_adapter/conversation/tests/local_facts.rs",
            225,
        ),
        (
            "src/app/tui_adapter/conversation/tests/presentation.rs",
            100,
        ),
        ("src/app/tui_adapter/conversation/tests/reply.rs", 75),
    ] {
        let source = fs::read_to_string(path).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "conversation test owner {path} exceeded its {line_budget}-line budget"
        );
    }
}
