pub(super) fn assert_runtime_observability_owners() {
    for target in [
        "src/adapters/sqlite/ledger_projection.rs",
        "src/adapters/sqlite/observability_projection.rs",
        "src/adapters/sqlite/transcript_projection.rs",
        "src/runtime_core/observability/facade.rs",
        "src/runtime_core/observability/html.rs",
        "src/runtime_core/observability/monitor.rs",
        "src/runtime_core/workflow/application/projection_barrier.rs",
    ] {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.7 observability owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    assert!(
        runtime_core
            .lines()
            .any(|line| line == "pub(crate) mod observability;"),
        "runtime observability owner is not crate-private"
    );
    let observability_mod = fs::read_to_string("src/runtime_core/observability/mod.rs").unwrap();
    for owner in ["facade", "html", "monitor"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            observability_mod.lines().any(|line| line == expected),
            "runtime observability child is not crate-private: {owner}"
        );
    }

    let facade = fs::read_to_string("src/runtime_core/observability/facade.rs").unwrap();
    assert!(
        facade.contains("trait ObservabilityProjectionPort"),
        "observability facade does not own the projection port"
    );
    assert!(
        facade.contains("trait CanonicalLedgerReadPort"),
        "observability facade does not own the canonical ledger read port"
    );
    assert!(
        facade.contains("trait CanonicalTranscriptReadPort")
            && facade.contains("trait CanonicalProjectionReadPort"),
        "observability facade does not own the canonical transcript projection port"
    );
    for record in [
        "struct StoreStatus",
        "struct MonitorProjectionSnapshot",
        "struct ModelRunMetric",
        "struct SessionHistoryEntry",
    ] {
        assert!(
            facade.contains(record),
            "observability facade is missing projection record: {record}"
        );
    }

    let monitor = fs::read_to_string("src/runtime_core/observability/monitor.rs").unwrap();
    for rule in [
        "trait MonitorQueryPort",
        "status_report",
        "models_report",
        "baseline_report",
        "optimize_report",
        "prune_report",
        "export_report",
    ] {
        assert!(
            monitor.contains(rule),
            "monitor owner is missing use case: {rule}"
        );
    }
    for (owner, line_budget, rules) in [
        (
            "format",
            75,
            &["fn display_optional_u64", "fn score_label"][..],
        ),
        (
            "metric",
            250,
            &["fn status_report", "fn models_report", "fn baseline_report"][..],
        ),
        (
            "policy",
            175,
            &["fn optimize_report", "fn prune_report"][..],
        ),
        ("report", 100, &["fn export_report", "fn html_report"][..]),
        (
            "tests",
            250,
            &[
                "status_report_is_rendered_from_port_data",
                "html_export_preserves_all_sections_when_queries_are_unavailable",
            ][..],
        ),
    ] {
        let relative = format!("monitor/{owner}.rs");
        assert!(
            monitor.contains(&relative),
            "monitor facade does not register {owner}"
        );
        let source =
            fs::read_to_string(format!("src/runtime_core/observability/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "monitor owner {owner} exceeded its {line_budget}-line budget"
        );
        for rule in rules {
            assert!(
                source.contains(rule),
                "monitor owner {owner} is missing contract: {rule}"
            );
        }
    }
    assert!(monitor.lines().count() < 100);
    let html = fs::read_to_string("src/runtime_core/observability/html.rs").unwrap();
    for rule in ["struct HtmlReportSnapshot", "fn render_report"] {
        assert!(
            html.contains(rule),
            "HTML monitor owner is missing contract: {rule}"
        );
    }
    assert!(html.lines().count() < 100);
    for (owner, line_budget, rules) in [
        (
            "template",
            175,
            &["Content-Security-Policy", "fn render_document_start"][..],
        ),
        (
            "sections",
            250,
            &["fn render_store_summary", "fn render_performance"][..],
        ),
        ("text", 125, &["fn safe_html_text", "fn escape_html"][..]),
        (
            "tests",
            200,
            &["report_is_self_contained", "empty_and_unavailable"][..],
        ),
    ] {
        let relative = format!("html/{owner}.rs");
        assert!(
            html.contains(&relative),
            "HTML monitor facade does not register {owner}"
        );
        let source =
            fs::read_to_string(format!("src/runtime_core/observability/{relative}")).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "HTML monitor owner {owner} exceeded its {line_budget}-line budget"
        );
        for rule in rules {
            assert!(
                source.contains(rule),
                "HTML monitor owner {owner} is missing contract: {rule}"
            );
        }
    }
}
