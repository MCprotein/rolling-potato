#[test]
fn v03713_benchmark_adapter_separates_regression_tests() {
    let adapter_path = "src/app/inference_adapter/benchmark.rs";
    let tests_path = "src/app/inference_adapter/benchmark/tests.rs";
    assert!(Path::new(adapter_path).is_file());
    assert!(Path::new(tests_path).is_file());

    let adapter = fs::read_to_string(adapter_path).unwrap();
    let tests = fs::read_to_string(tests_path).unwrap();
    assert!(
        adapter.contains("#[path = \"benchmark/tests.rs\"]"),
        "benchmark adapter does not register its regression test owner"
    );
    for regression in [
        "fn validates_fixture_metadata(",
        "fn executable_run_records_local_score_without_prompt_text(",
        "fn rejects_raw_prompt_field(",
        "fn canonical_model_adoption_fixture_is_valid(",
    ] {
        assert!(
            tests.contains(regression),
            "benchmark test owner is missing regression: {regression}"
        );
        assert!(
            !adapter.contains(regression),
            "benchmark production adapter still owns regression: {regression}"
        );
    }
    assert!(adapter.lines().count() < 350);
    assert!(tests.lines().count() < 450);
}
