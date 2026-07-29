#[test]
fn web_search_open_find_have_separate_bounded_owners() {
    let adapter_facade = fs::read_to_string("src/adapters/web_search.rs").unwrap();
    let app_facade = fs::read_to_string("src/app/web_search_adapter.rs").unwrap();
    let tui_facade = fs::read_to_string("src/app/tui_adapter.rs").unwrap();
    let tui_runtime = fs::read_to_string("src/app/tui_adapter/runtime.rs").unwrap();
    let tui_request = fs::read_to_string("src/app/tui_adapter/runtime/request.rs").unwrap();
    let tui_request_support =
        fs::read_to_string("src/app/tui_adapter/runtime/request/support.rs").unwrap();
    let web_tools = fs::read_to_string("src/app/tui_adapter/web_tools.rs").unwrap();
    let tui_controller = fs::read_to_string("src/surfaces/tui/controller.rs").unwrap();
    let tui_command_dispatch =
        fs::read_to_string("src/surfaces/tui/controller/command_dispatch.rs").unwrap();
    let tui_workspace_dispatch =
        fs::read_to_string("src/surfaces/tui/controller/command_dispatch/workspace.rs").unwrap();
    let tui_web_dispatch =
        fs::read_to_string("src/surfaces/tui/controller/command_dispatch/web.rs").unwrap();
    let tui_bridge = fs::read_to_string("src/surfaces/tui/runtime_bridge.rs").unwrap();
    let transport = fs::read_to_string("src/adapters/web_search/transport.rs").unwrap();
    let page_parser = fs::read_to_string("src/adapters/web_search/page.rs").unwrap();
    let agent_turn = fs::read_to_string("src/runtime_core/agent.rs").unwrap();

    for path in [
        "src/adapters/web_search/evidence.rs",
        "src/adapters/web_search/find.rs",
        "src/adapters/web_search/html.rs",
        "src/adapters/web_search/page.rs",
        "src/adapters/web_search/policy.rs",
        "src/adapters/web_search/tests.rs",
        "src/adapters/web_search/tests/browser_policy.rs",
        "src/adapters/web_search/tests/open_find.rs",
        "src/adapters/web_search/tests/search.rs",
        "src/adapters/web_search/transport.rs",
        "src/app/web_search_adapter/answer_contract.rs",
        "src/app/web_search_adapter/answer_binding.rs",
        "src/app/web_search_adapter/answer_binding/presentation.rs",
        "src/app/web_search_adapter/answer_binding/sanitize.rs",
        "src/app/web_search_adapter/answer_binding/tests.rs",
        "src/app/web_search_adapter/grounded_fallback.rs",
        "src/app/web_search_adapter/page_session.rs",
        "src/app/web_search_adapter/page_tools.rs",
        "src/app/web_search_adapter/page_tools/find.rs",
        "src/app/web_search_adapter/page_tools/open.rs",
        "src/app/web_search_adapter/research.rs",
        "src/app/web_search_adapter/research/fallback.rs",
        "src/app/web_search_adapter/research/session.rs",
        "src/app/web_search_adapter/research/tests.rs",
        "src/app/web_search_adapter/research/types.rs",
        "src/app/web_search_adapter/research_flow.rs",
        "src/app/web_search_adapter/routing/grounding_policy.rs",
        "src/app/web_search_adapter/routing/grounding_policy/features.rs",
        "src/app/web_search_adapter/routing/grounding_policy/query_plan.rs",
        "src/app/web_search_adapter/routing/grounding_policy/tests.rs",
        "src/app/web_search_adapter/routing/page_intent.rs",
        "src/app/web_search_adapter/routing/protocol.rs",
        "src/app/web_search_adapter/routing/query.rs",
        "src/app/web_search_adapter/routing/query/context.rs",
        "src/app/web_search_adapter/routing/query/sanitize.rs",
        "src/app/web_search_adapter/routing/query/tests.rs",
        "src/app/web_search_adapter/routing.rs",
        "src/app/web_search_adapter/routing/tests.rs",
        "src/app/web_search_adapter/routing/text.rs",
        "src/app/web_search_adapter/routing/web_policy.rs",
        "src/app/tui_adapter/runtime/web_sources.rs",
        "src/app/tui_adapter/runtime/request/support.rs",
        "src/app/tui_adapter/web_tools.rs",
        "src/runtime_core/agent.rs",
        "src/surfaces/tui/controller/command_dispatch/web.rs",
        "src/surfaces/tui/controller/command_dispatch/workspace.rs",
        "src/surfaces/tui/controller/source_selection.rs",
    ] {
        assert!(Path::new(path).is_file(), "missing web tool owner: {path}");
    }
    for module in ["evidence", "find", "html", "page", "policy", "transport"] {
        assert!(
            adapter_facade
                .lines()
                .any(|line| line == format!("mod {module};")),
            "web adapter facade does not register {module}"
        );
    }
    assert!(adapter_facade.lines().any(|line| line == "mod tests;"));
    for (path, maximum_lines) in [
        ("src/adapters/web_search.rs", 250),
        ("src/adapters/web_search/tests.rs", 50),
        ("src/adapters/web_search/tests/browser_policy.rs", 175),
        ("src/adapters/web_search/tests/open_find.rs", 175),
        ("src/adapters/web_search/tests/search.rs", 225),
    ] {
        assert!(
            fs::read_to_string(path).unwrap().lines().count() < maximum_lines,
            "web adapter owner regrew beyond its boundary: {path}"
        );
    }
    for module in [
        "answer_contract",
        "answer_binding",
        "grounded_fallback",
        "page_session",
        "page_tools",
        "research",
        "research_flow",
        "routing",
    ] {
        assert!(
            app_facade
                .lines()
                .any(|line| line == format!("mod {module};")),
            "web application facade does not register {module}"
        );
    }
    assert!(tui_facade.lines().any(|line| line == "mod web_tools;"));
    assert!(tui_runtime.lines().any(|line| line == "mod web_sources;"));
    assert!(tui_controller.contains("mod source_selection;"));
    assert!(tui_command_dispatch.contains("mod workspace;"));
    assert!(tui_command_dispatch.contains("mod web;"));
    assert!(tui_workspace_dispatch.contains("[\"/sources\"]"));
    for command in ["[\"/search\"]", "[\"/open\"]", "[\"/find\"]"] {
        assert!(
            tui_web_dispatch.contains(command),
            "web command owner is missing {command}"
        );
        assert!(
            !tui_command_dispatch.contains(command),
            "TUI command facade still owns {command}"
        );
    }
    assert!(tui_web_dispatch.lines().count() < 125);
    assert!(tui_bridge.contains("struct TuiWebSourceOption"));
    assert!(tui_request.contains("web_search_adapter::route_tool_request"));
    assert!(tui_request.contains("execute_web_turn("));
    assert!(tui_request.lines().any(|line| line == "mod support;"));
    assert!(tui_request_support.contains("web_tools::observe"));
    assert!(tui_request_support.contains("web_tools::answer"));
    assert!(tui_request_support.contains("fn required_context_limit("));
    assert!(!web_tools.contains("route_tool_request"));
    assert!(agent_turn.contains("TURN_DECISION_JSON_SCHEMA"));
    assert!(agent_turn.contains("enum AgentTurnDecision"));
    assert!(agent_turn.contains("fn parse_turn_decision("));
    assert!(!agent_turn.contains("WEB TOOL:"));
    assert!(transport.contains("ureq::Agent::with_parts"));
    assert!(transport.contains("PublicWebResolver"));
    assert!(page_parser.contains("scan_html"));
    assert!(!page_parser.contains("replace_range"));
    let routing = fs::read_to_string("src/app/web_search_adapter/routing.rs").unwrap();
    let routing_protocol =
        fs::read_to_string("src/app/web_search_adapter/routing/protocol.rs").unwrap();
    let routing_page_intent =
        fs::read_to_string("src/app/web_search_adapter/routing/page_intent.rs").unwrap();
    let routing_web_policy =
        fs::read_to_string("src/app/web_search_adapter/routing/web_policy.rs").unwrap();
    let routing_query = fs::read_to_string("src/app/web_search_adapter/routing/query.rs").unwrap();
    let routing_query_context =
        fs::read_to_string("src/app/web_search_adapter/routing/query/context.rs").unwrap();
    let routing_query_sanitize =
        fs::read_to_string("src/app/web_search_adapter/routing/query/sanitize.rs").unwrap();
    assert!(routing.lines().any(|line| line == "mod protocol;"));
    assert!(routing.lines().any(|line| line == "mod page_intent;"));
    assert!(routing.lines().any(|line| line == "mod query;"));
    assert!(routing.lines().any(|line| line == "mod web_policy;"));
    assert!(routing_protocol.contains("fn route_tool_request("));
    assert!(routing_page_intent.contains("fn route_current_page_find("));
    assert!(tui_request.contains("web_search_adapter::route_current_page_find"));
    assert!(!routing_protocol.contains("WEB TOOL:"));
    assert!(routing_query.contains("fn contextualize_search_input("));
    assert!(routing_query.lines().any(|line| line == "mod context;"));
    assert!(routing_query.lines().any(|line| line == "mod sanitize;"));
    assert!(routing_web_policy.contains("fn web_disabled("));
    let research = fs::read_to_string("src/app/web_search_adapter/research.rs").unwrap();
    let research_fallback =
        fs::read_to_string("src/app/web_search_adapter/research/fallback.rs").unwrap();
    let research_session =
        fs::read_to_string("src/app/web_search_adapter/research/session.rs").unwrap();
    let research_tests =
        fs::read_to_string("src/app/web_search_adapter/research/tests.rs").unwrap();
    let research_types =
        fs::read_to_string("src/app/web_search_adapter/research/types.rs").unwrap();
    for module in ["mod fallback;", "mod session;", "mod types;"] {
        assert!(
            research.lines().any(|line| line == module),
            "web research facade does not register {module}"
        );
    }
    assert!(research.contains("#[path = \"research/tests.rs\"]"));
    for (owner, responsibility) in [
        (research_types.as_str(), "enum WebResearchStep"),
        (research_types.as_str(), "struct WebResearchBudget"),
        (research_types.as_str(), "enum WebResearchTerminal"),
        (research_session.as_str(), "struct WebResearchSession"),
        (research_session.as_str(), "fn admit("),
        (research_session.as_str(), "fn take_evidence("),
        (research_session.as_str(), "fn deterministic_fallback("),
        (
            research_fallback.as_str(),
            "fn deterministic_freshness_fallback_for_context(",
        ),
        (
            research_tests.as_str(),
            "fn routing_budget_stops_at_search_revision_and_document_find_limits(",
        ),
    ] {
        assert!(
            owner.contains(responsibility),
            "web research responsibility owner is missing: {responsibility}"
        );
        assert!(
            !research.contains(responsibility),
            "web research facade still owns behavior: {responsibility}"
        );
    }
    for (owner, maximum_lines, path) in [
        (research.as_str(), 50, "research.rs"),
        (research_fallback.as_str(), 75, "research/fallback.rs"),
        (research_session.as_str(), 275, "research/session.rs"),
        (research_tests.as_str(), 375, "research/tests.rs"),
        (research_types.as_str(), 175, "research/types.rs"),
    ] {
        assert!(
            owner.lines().count() < maximum_lines,
            "web research owner regrew beyond its boundary: {path}"
        );
    }
    for pure_owner in [
        "src/adapters/web_search/find.rs",
        "src/adapters/web_search/page.rs",
        "src/adapters/web_search/evidence.rs",
    ] {
        assert!(
            !fs::read_to_string(pure_owner)
                .unwrap()
                .contains("ureq::Agent"),
            "{pure_owner} must stay transport-free"
        );
    }
    assert!(app_facade.lines().count() < 400);
    assert!(
        fs::read_to_string("src/app/web_search_adapter/answer_binding.rs")
            .unwrap()
            .lines()
            .count()
            < 300
    );
    assert!(
        fs::read_to_string("src/app/web_search_adapter/answer_contract.rs")
            .unwrap()
            .lines()
            .count()
            < 175
    );
    assert!(
        fs::read_to_string("src/app/web_search_adapter/page_session.rs")
            .unwrap()
            .lines()
            .count()
            < 150
    );
    assert!(routing.lines().count() < 225);
    assert!(routing_protocol.lines().count() < 150);
    assert!(routing_page_intent.lines().count() < 75);
    assert!(routing_query.lines().count() < 225);
    assert!(routing_query_context.lines().count() < 100);
    assert!(routing_query_sanitize.lines().count() < 150);
    assert!(routing_web_policy.lines().count() < 100);
    assert!(
        fs::read_to_string("src/app/web_search_adapter/page_tools.rs")
            .unwrap()
            .lines()
            .count()
            < 25
    );
    assert!(
        fs::read_to_string("src/app/web_search_adapter/page_tools/find.rs")
            .unwrap()
            .lines()
            .count()
            < 125
    );
    assert!(
        fs::read_to_string("src/app/web_search_adapter/page_tools/open.rs")
            .unwrap()
            .lines()
            .count()
            < 125
    );
    let research_flow = fs::read_to_string("src/app/web_search_adapter/research_flow.rs").unwrap();
    let research_flow_tests =
        fs::read_to_string("src/app/web_search_adapter/research_flow/tests.rs").unwrap();
    assert!(research_flow.contains("#[path = \"research_flow/tests.rs\"]"));
    assert!(research_flow_tests
        .contains("opened_primary_document_overrides_conflicting_search_snippet"));
    assert!(research_flow.contains("search.sources.iter().take(3)"));
    assert!(research_flow.contains("supporting_passages("));
    assert!(research_flow.lines().count() < 350);
    assert!(research_flow_tests.lines().count() < 225);
    assert!(tui_runtime.lines().count() <= 200);
    assert!(tui_request.lines().count() < 150);
    assert!(tui_request_support.lines().count() < 75);
}

#[test]
fn web_browser_documentation_contract_is_wired_into_candidate_preflight() {
    let preflight = fs::read_to_string("scripts/ci/verify-pr-candidate-preflight.sh").unwrap();
    let docs_contract = fs::read_to_string("scripts/ci/verify-web-browser-docs.sh").unwrap();

    assert!(preflight.contains("scripts/ci/verify-web-browser-docs.sh"));
    for path in [
        "README.md",
        "README.ko.md",
        "PRIVACY.md",
        "docs/ko/PRIVACY.md",
        "SECURITY.md",
        "docs/ko/SECURITY.md",
        "docs/threat-model.md",
        "docs/ko/threat-model.md",
        "docs/tui.md",
        "docs/ko/tui.md",
        "docs/current-capabilities.md",
        "docs/ko/current-capabilities.md",
        "docs/runtime-architecture.md",
        "docs/ko/runtime-architecture.md",
        "docs/v0.50-web-research-browser-plan.md",
        "docs/ko/v0.50-web-research-browser-plan.md",
    ] {
        assert!(
            docs_contract.contains(path),
            "web/browser docs contract does not cover {path}"
        );
    }
    for marker in [
        "v0.50.0",
        "Restricted Browser Abuse",
        "제한된 브라우저 오용",
        "loopback HTTPS CONNECT",
        "Web Research and Restricted Browser",
        "웹 연구와 제한된 브라우저",
    ] {
        assert!(
            docs_contract.contains(marker),
            "web/browser docs contract does not assert {marker}"
        );
    }
}
