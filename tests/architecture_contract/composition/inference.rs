#[test]
fn inference_composition_has_bounded_command_family_owners() {
    let facade_path = "src/composition/inference.rs";
    let facade = fs::read_to_string(facade_path).unwrap();
    let owners = [
        (
            "ports",
            100,
            &[
                "trait BenchmarkCommandPort",
                "trait BackendCommandPort",
                "trait ModelCommandPort",
            ][..],
        ),
        ("benchmark", 50, &["fn run_benchmark("][..]),
        ("backend", 75, &["fn run_backend("][..]),
        ("model", 75, &["fn run_model("][..]),
        (
            "tests",
            350,
            &[
                "fn backend_start_resolves_default_model_before_start(",
                "fn model_install_has_no_command_output(",
            ][..],
        ),
    ];

    assert!(
        facade.lines().count() < 50,
        "inference composition facade regrew beyond stable exports"
    );
    for (owner, line_budget, responsibilities) in owners {
        let relative = format!("inference/{owner}.rs");
        assert!(
            facade.contains(&relative),
            "inference composition facade does not register {owner}"
        );
        let source = fs::read_to_string(format!("src/composition/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "inference composition owner {owner} exceeded its {line_budget}-line budget"
        );
        for responsibility in responsibilities {
            assert!(
                source.contains(responsibility),
                "inference composition owner {owner} is missing {responsibility}"
            );
            assert!(
                !facade.contains(responsibility),
                "inference composition facade still owns {responsibility}"
            );
        }
    }
}
