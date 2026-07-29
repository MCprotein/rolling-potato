use super::*;

#[test]
fn session_memory_review_fixes_keep_separate_bounded_owners() {
    let tui_runtime = fs::read_to_string("src/app/tui_adapter/runtime.rs").unwrap();
    let tui_request = fs::read_to_string("src/app/tui_adapter/runtime/request.rs").unwrap();
    let session_memory = fs::read_to_string("src/app/tui_adapter/session_memory.rs").unwrap();
    let session_event_codec =
        fs::read_to_string("src/app/tui_adapter/session_memory/event_codec.rs").unwrap();
    let session_restoration =
        fs::read_to_string("src/app/tui_adapter/session_memory/restoration.rs").unwrap();
    let session_tests = fs::read_to_string("src/app/tui_adapter/session_memory/tests.rs").unwrap();
    let session_restoration_tests =
        fs::read_to_string("src/app/tui_adapter/session_memory/tests/restoration.rs").unwrap();
    let intent_tests = fs::read_to_string("src/app/intent_adapter/tests.rs").unwrap();
    let prompt_budget_tests =
        fs::read_to_string("src/app/intent_adapter/tests/prompt_budget.rs").unwrap();
    let context = fs::read_to_string("src/runtime_core/knowledge/context.rs").unwrap();
    let compaction = fs::read_to_string("src/runtime_core/knowledge/compaction.rs").unwrap();
    let recent_tail =
        fs::read_to_string("src/runtime_core/knowledge/compaction/recent_tail.rs").unwrap();
    let native_terminal = fs::read_to_string("tests/surfaces/native_terminal.rs").unwrap();

    assert!(session_memory.contains("#[path = \"session_memory/tests.rs\"]"));
    assert!(session_memory
        .lines()
        .any(|line| line == "mod event_codec;"));
    assert!(session_memory
        .lines()
        .any(|line| line == "mod restoration;"));
    assert!(session_event_codec.contains("fn parse_conversation_event("));
    assert!(session_restoration.contains("fn load_for_session("));
    assert!(session_tests.contains("#[path = \"tests/restoration.rs\"]"));
    assert!(session_tests
        .lines()
        .any(|line| line == "mod restoration_tests;"));
    assert!(session_restoration_tests
        .contains("fn web_grounding_is_bounded_and_restored_for_followups_after_resume("));
    assert!(intent_tests.contains("#[path = \"tests/prompt_budget.rs\"]"));
    assert!(compaction.lines().any(|line| line == "mod recent_tail;"));

    for responsibility in [
        "fn reset_is_a_unique_causal_head_for_repeated_questions(",
        "fn reset_discards_an_orphan_user_before_a_later_model_record(",
        "fn coding_exchange_is_canonical_and_prompt_history_keeps_budgetable_pairs(",
    ] {
        assert!(
            session_tests.contains(responsibility),
            "session-memory regression owner is missing: {responsibility}"
        );
        assert!(
            !session_memory.contains(responsibility),
            "session-memory production owner contains regression test: {responsibility}"
        );
    }
    assert!(tui_runtime.contains("super::session_memory::record_exchange("));
    assert!(!tui_request.contains("TranscriptOwner"));
    assert!(
        !native_terminal.contains("confirm_picker(&mut terminal, \"세션 선택 확인\")"),
        "session resume는 workflow dispatch fault probe로 재사용하면 안 됩니다."
    );

    for responsibility in [
        "fn imported_skill_instructions_are_bounded_by_runtime_contract(",
        "fn agent_loop_prompt_bounds_resume_and_sources_to_the_active_runtime_window(",
    ] {
        assert!(
            prompt_budget_tests.contains(responsibility),
            "agent prompt regression owner is missing: {responsibility}"
        );
        assert!(
            !intent_tests.contains(responsibility),
            "intent regression facade contains prompt-budget test: {responsibility}"
        );
    }
    for responsibility in [
        "struct AgentPromptBudget",
        "struct AgentPromptParts",
        "fn assemble_agent_prompt(",
    ] {
        assert!(
            context.contains(responsibility),
            "context owner is missing agent prompt policy: {responsibility}"
        );
    }

    for responsibility in [
        "fn select_recent_tail(",
        "fn exchange_ranges(",
        "fn bounded_single_exchange(",
    ] {
        assert!(
            recent_tail.contains(responsibility),
            "recent-tail owner is missing: {responsibility}"
        );
        assert!(
            !compaction.contains(responsibility),
            "compaction facade still owns recent-tail policy: {responsibility}"
        );
    }

    assert!(session_memory.lines().count() < 225);
    assert!(session_tests.lines().count() < 225);
    assert!(session_event_codec.lines().count() < 125);
    assert!(session_restoration.lines().count() < 125);
    assert!(session_restoration_tests.lines().count() < 175);
    assert!(intent_tests.lines().count() < 325);
    assert!(prompt_budget_tests.lines().count() < 175);
    assert!(compaction.lines().count() < 550);
    assert!(recent_tail.lines().count() < 350);
}

fn dependency_edges(root: &Object) -> (BTreeSet<String>, BTreeSet<(String, String)>) {
    let contract = field_object(root, "dependency_contract", "map");
    let roots = string_array(
        field_array(contract, "roots", "map.dependency_contract"),
        "map.dependency_contract.roots",
    )
    .into_iter()
    .collect::<BTreeSet<_>>();
    let mut edges = BTreeSet::new();
    for (index, value) in field_array(contract, "allowed_edges", "map.dependency_contract")
        .iter()
        .enumerate()
    {
        let context = format!("map.dependency_contract.allowed_edges[{index}]");
        let edge = as_object(value, &context);
        edges.insert((
            field_string(edge, "from", &context).to_owned(),
            field_string(edge, "to", &context).to_owned(),
        ));
    }
    assert!(
        field_array(contract, "exceptions", "map.dependency_contract").is_empty(),
        "v0.37.1 dependency contract must not begin with exceptions"
    );
    (roots, edges)
}

fn direct_dependencies() -> BTreeSet<String> {
    let cargo = fs::read_to_string("Cargo.toml").expect("Cargo.toml must be readable");
    let mut in_dependencies = false;
    let mut dependencies = BTreeSet::new();
    for line in cargo.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dependencies = line == "[dependencies]";
            continue;
        }
        if in_dependencies && !line.is_empty() && !line.starts_with('#') {
            let name = line
                .split_once('=')
                .map(|(name, _)| name.trim())
                .unwrap_or_else(|| panic!("invalid dependency declaration: {line}"));
            dependencies.insert(name.to_owned());
        }
    }
    dependencies
}

#[test]
fn dependency_contract_rejects_forbidden_imports_and_new_parser_crates() {
    let map = load_map();
    let root = as_object(&map, "map");
    let (roots, edges) = dependency_edges(root);
    assert_eq!(
        roots,
        ARCHITECTURE_ROOTS.into_iter().map(str::to_owned).collect()
    );
    let required_edges = BTreeSet::from([
        ("app".to_owned(), "composition".to_owned()),
        ("app".to_owned(), "surfaces".to_owned()),
        ("app".to_owned(), "runtime_core".to_owned()),
        ("app".to_owned(), "adapters".to_owned()),
        ("app".to_owned(), "foundation".to_owned()),
        ("composition".to_owned(), "surfaces".to_owned()),
        ("composition".to_owned(), "runtime_core".to_owned()),
        ("composition".to_owned(), "adapters".to_owned()),
        ("composition".to_owned(), "foundation".to_owned()),
        ("surfaces".to_owned(), "runtime_core".to_owned()),
        ("surfaces".to_owned(), "foundation".to_owned()),
        ("runtime_core".to_owned(), "foundation".to_owned()),
        ("adapters".to_owned(), "runtime_core".to_owned()),
        ("adapters".to_owned(), "foundation".to_owned()),
    ]);
    assert_eq!(
        edges, required_edges,
        "dependency contract was weakened or widened"
    );

    for source_root in &roots {
        for path in collect_rust_files(&format!("src/{source_root}")) {
            let source = fs::read_to_string(&path).unwrap();
            for (line_index, line) in source.lines().enumerate() {
                let line = line.trim_start();
                let Some(import) = line
                    .strip_prefix("use crate::")
                    .or_else(|| line.strip_prefix("pub(crate) use crate::"))
                else {
                    continue;
                };
                let target_root = import.split([':', ';', '{']).next().unwrap_or("");
                assert!(
                    roots.contains(target_root),
                    "{path}:{} imports concrete legacy root {target_root}",
                    line_index + 1
                );
                assert!(
                    source_root == target_root
                        || edges.contains(&(source_root.clone(), target_root.to_owned())),
                    "{path}:{} has forbidden dependency {source_root} -> {target_root}",
                    line_index + 1
                );
            }
        }
    }

    assert_eq!(
        direct_dependencies(),
        BTreeSet::from([
            "flate2".to_owned(),
            "rusqlite".to_owned(),
            "sha2".to_owned(),
            "tar".to_owned(),
            "ureq".to_owned(),
            "zip".to_owned(),
        ]),
        "v0.37.1 must not add a parser or architecture-test dependency"
    );
}

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
        "src/app/web_search_adapter/research_flow.rs",
        "src/app/web_search_adapter/routing/grounding_policy.rs",
        "src/app/web_search_adapter/routing/grounding_policy/features.rs",
        "src/app/web_search_adapter/routing/grounding_policy/query_plan.rs",
        "src/app/web_search_adapter/routing/grounding_policy/tests.rs",
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
    for module in [
        "answer_contract",
        "answer_binding",
        "grounded_fallback",
        "page_session",
        "page_tools",
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
    assert!(tui_controller.contains("[\"/sources\"]"));
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
    let routing_web_policy =
        fs::read_to_string("src/app/web_search_adapter/routing/web_policy.rs").unwrap();
    let routing_query = fs::read_to_string("src/app/web_search_adapter/routing/query.rs").unwrap();
    let routing_query_context =
        fs::read_to_string("src/app/web_search_adapter/routing/query/context.rs").unwrap();
    let routing_query_sanitize =
        fs::read_to_string("src/app/web_search_adapter/routing/query/sanitize.rs").unwrap();
    assert!(routing.lines().any(|line| line == "mod protocol;"));
    assert!(routing.lines().any(|line| line == "mod query;"));
    assert!(routing.lines().any(|line| line == "mod web_policy;"));
    assert!(routing_protocol.contains("fn route_tool_request("));
    assert!(!routing_protocol.contains("WEB TOOL:"));
    assert!(routing_query.contains("fn contextualize_search_input("));
    assert!(routing_query.lines().any(|line| line == "mod context;"));
    assert!(routing_query.lines().any(|line| line == "mod sanitize;"));
    assert!(routing_web_policy.contains("fn web_disabled("));
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
    let tui_request = fs::read_to_string("src/app/tui_adapter/runtime/request.rs").unwrap();

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
    }
    assert!(conversation.contains("history_only_secret_cannot_become_network_tool_input"));
    assert!(conversation_decision.contains("RequestDecision::BrowserTool"));
    assert!(conversation_decision.contains("generate_structured_candidate_for_user"));
    assert!(conversation_decision.contains("TURN_DECISION_JSON_SCHEMA"));
    assert!(conversation_decision.contains("deterministic_browser_fallback"));
    assert!(tui_request.contains("RequestDecision::BrowserTool"));

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
    assert!(conversation.lines().count() < 500);
    assert!(conversation_decision.lines().count() < 300);
    assert!(conversation_local_facts.lines().count() < 300);
    assert!(conversation_presentation.lines().count() < 125);
    assert!(conversation_reply.lines().count() < 125);
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
