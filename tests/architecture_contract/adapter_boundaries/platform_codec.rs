#[test]
fn v03713_workflow_record_separates_compatibility_codec() {
    let record_path = "src/runtime_core/workflow/storage_compat/record.rs";
    let codec_path = "src/runtime_core/workflow/storage_compat/record/codec.rs";
    assert!(Path::new(record_path).is_file());
    assert!(Path::new(codec_path).is_file());

    let record = fs::read_to_string(record_path).unwrap();
    let codec = fs::read_to_string(codec_path).unwrap();
    assert!(record.contains("#[path = \"record/codec.rs\"]"));
    assert!(record.contains("pub struct WorkflowRecord"));
    assert!(record.contains("impl WorkflowRecord"));
    for responsibility in [
        "render_pointer",
        "parse_pointer",
        "snapshot_schema",
        "parse_snapshot",
        "payload",
        "render",
    ] {
        assert!(
            codec.contains(responsibility),
            "workflow record codec is missing responsibility: {responsibility}"
        );
        assert!(
            !record.contains(&format!("fn {responsibility}")),
            "workflow record model still owns codec behavior: {responsibility}"
        );
    }
    for (owner, line_budget, responsibilities) in [
        (
            "versions",
            150,
            &[
                "const WORKFLOW_V2_KEYS",
                "const WORKFLOW_V3_KEYS",
                "const WORKFLOW_V4_KEYS",
            ][..],
        ),
        (
            "pointer",
            125,
            &["fn render_pointer", "fn parse_pointer"][..],
        ),
        (
            "snapshot",
            225,
            &["fn snapshot_schema", "fn parse_snapshot"][..],
        ),
        (
            "payload",
            175,
            &["fn payload", "fn payload_v2", "fn payload_v3"][..],
        ),
        (
            "render",
            200,
            &["fn render", "fn render_v2", "fn render_v3"][..],
        ),
    ] {
        let relative = format!("codec/{owner}.rs");
        assert!(
            codec.contains(&relative),
            "workflow record codec facade does not register {owner}"
        );
        let source = fs::read_to_string(format!(
            "src/runtime_core/workflow/storage_compat/record/{relative}"
        ))
        .unwrap();
        assert!(
            source.lines().count() < line_budget,
            "workflow record codec owner {owner} exceeded its {line_budget}-line budget"
        );
        for responsibility in responsibilities {
            assert!(
                source.contains(responsibility),
                "workflow record codec owner {owner} is missing: {responsibility}"
            );
        }
    }
    assert!(record.lines().count() < 150);
    assert!(codec.lines().count() < 75);
}

#[test]
fn v03713_platform_fixtures_are_grouped_under_support_boundary() {
    for name in [
        "fake_sidecar.rs",
        "native_terminal.rs",
        "native_terminal_probe.rs",
    ] {
        assert!(!Path::new(&format!("tests/support/{name}")).exists());
        assert!(Path::new(&format!("tests/support/platform/{name}")).is_file());
    }

    let harness = fs::read_to_string("tests/surfaces.rs").unwrap();
    assert!(harness.contains("support/platform/native_terminal.rs"));
    assert!(harness.contains("surfaces/interactive_tui.rs"));
    assert!(harness.contains("surfaces/native_terminal.rs"));
    assert!(!Path::new("tests/platform.rs").exists());
    assert!(!Path::new("tests/platform").exists());

    let native_terminal = fs::read_to_string("tests/support/platform/native_terminal.rs").unwrap();
    let owners = [
        ("capture", 125),
        ("fixture", 450),
        ("process", 150),
        ("trace", 50),
        ("unix", 600),
        ("windows", 50),
    ];
    for (owner, line_budget) in owners {
        let relative = format!("native_terminal/{owner}.rs");
        assert!(
            native_terminal.contains(&relative),
            "native terminal facade does not register {owner}"
        );
        let source = fs::read_to_string(format!("tests/support/platform/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "native terminal owner {owner} exceeded its {line_budget}-line budget"
        );
    }
    assert!(native_terminal.lines().count() < 75);

    let windows = fs::read_to_string("tests/support/platform/native_terminal/windows.rs").unwrap();
    for (owner, line_budget) in [
        ("ffi", 175),
        ("session", 250),
        ("pty", 225),
        ("process", 200),
    ] {
        let relative = format!("windows/{owner}.rs");
        assert!(
            windows.contains(&relative),
            "Windows terminal facade does not register {owner}"
        );
        let source =
            fs::read_to_string(format!("tests/support/platform/native_terminal/{relative}"))
                .unwrap();
        assert!(
            source.lines().count() < line_budget,
            "Windows terminal owner {owner} exceeded its {line_budget}-line budget"
        );
    }

    let surface = fs::read_to_string("tests/surfaces/native_terminal.rs").unwrap();
    let journey_owners = [
        ("adapter_matrix", 25),
        ("interaction", 200),
        ("lifecycle", 150),
        ("web", 150),
    ];
    for (owner, line_budget) in journey_owners {
        let relative = format!("native_terminal/{owner}.rs");
        assert!(
            surface.contains(&relative),
            "native terminal surface does not register {owner}"
        );
        let source = fs::read_to_string(format!("tests/surfaces/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "native terminal journey {owner} exceeded its {line_budget}-line budget"
        );
    }
    let adapter_matrix =
        fs::read_to_string("tests/surfaces/native_terminal/adapter_matrix.rs").unwrap();
    for (owner, line_budget) in [
        ("full_adapter", 400),
        ("outcome_oracles", 200),
        ("state_oracles", 350),
    ] {
        let relative = format!("adapter_matrix/{owner}.rs");
        assert!(
            adapter_matrix.contains(&relative),
            "native terminal adapter matrix does not register {owner}"
        );
        let source =
            fs::read_to_string(format!("tests/surfaces/native_terminal/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "native terminal adapter matrix owner {owner} exceeded its {line_budget}-line budget"
        );
    }
    assert!(surface.lines().count() < 150);
}
