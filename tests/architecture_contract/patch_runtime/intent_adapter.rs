#[test]
fn intent_adapter_has_bounded_router_handler_lifecycle_owners() {
    let facade_path = "src/app/intent_adapter.rs";
    let facade = fs::read_to_string(facade_path).unwrap();
    let owners = [
        (
            "routing",
            100,
            &["fn classify(", "fn run_skill_report("][..],
        ),
        (
            "prompt",
            125,
            &["fn agent_loop_prompt(", "fn agent_loop_prompt_for_context("][..],
        ),
        (
            "context_requirements",
            100,
            &["fn available_context_labels("][..],
        ),
        (
            "lifecycle",
            125,
            &["fn fail_skill_workflow(", "fn dispatch_skill_hook("][..],
        ),
        ("outcomes", 175, &["fn record_non_mutating_outcomes("][..]),
        (
            "reporting",
            100,
            &["fn render_non_mutating_report(", "fn model_answer("][..],
        ),
    ];

    assert!(
        facade.lines().count() < 75,
        "intent adapter facade regrew beyond stable routing exports"
    );
    for (owner, line_budget, responsibilities) in owners {
        let relative = format!("intent_adapter/{owner}.rs");
        assert!(
            facade.lines().any(|line| line == format!("mod {owner};")),
            "intent facade does not register {owner}"
        );
        let source = fs::read_to_string(format!("src/app/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "intent owner {owner} exceeded its {line_budget}-line budget"
        );
        for responsibility in responsibilities {
            assert!(
                source.contains(responsibility),
                "intent owner {owner} is missing {responsibility}"
            );
            assert!(
                !facade.contains(responsibility),
                "intent facade still owns {responsibility}"
            );
        }
    }

    assert!(
        facade.lines().any(|line| line == "mod execution;"),
        "intent facade no longer registers its execution owner"
    );
    assert!(
        facade.contains("#[path = \"intent_adapter/tests.rs\"]"),
        "intent facade no longer registers its regression-test owner"
    );
}
