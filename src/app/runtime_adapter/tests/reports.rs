#[test]
fn docs_recovery_outcome_oracles_are_bilingual_and_exact() {
    let english = include_str!("../../../../docs/tui.md");
    let korean = include_str!("../../../../docs/ko/tui.md");
    let contract = |document: &str| {
        document
            .split_once("<!-- TUI-READ-CONTRACT:START -->\n")
            .and_then(|(_, tail)| tail.split_once("\n<!-- TUI-READ-CONTRACT:END -->"))
            .map(|(body, _)| body.to_string())
            .expect("exact TUI read contract markers")
    };
    assert_eq!(
        contract(english),
        "The eight views (`overview`, `monitor`, `sessions`, `transcript`, `tool-output`,\n`approvals`, `diff`, and `evidence`) use view-specific item, byte, scan, line, and\npagination bounds. Every page carries canonical current/workflow revision and hash,\nledger sequence and hash, relevant content or transcript hash, projection watermark,\nvalidation time, and one typed continuation: `complete`, `next-page`, `truncated`,\n`unavailable`, or `redacted`. SQLite is a derived metrics/freshness projection only;\nfreshness is exactly `fresh`, `stale`, `projection-lag`, or `unavailable`. Read paths do\nnot acquire mutation leases, repair state, write validation gaps, or admit corrupt,\nunbound, SQLite-only, or directory-scan-only candidates."
    );
    assert_eq!(
        contract(korean),
        "8개 view(`overview`, `monitor`, `sessions`, `transcript`, `tool-output`, `approvals`,\n`diff`, `evidence`)는 view별 item, byte, scan, line, pagination 상한을 적용합니다. 모든\npage는 canonical current/workflow revision과 hash, ledger sequence와 hash, 관련 content\n또는 transcript hash, projection watermark, validation time, 그리고 `complete`,\n`next-page`, `truncated`, `unavailable`, `redacted` 중 하나의 typed continuation을\n포함합니다. SQLite는 파생된 metrics/freshness projection일 뿐이며 freshness 표기는 정확히\n`fresh`, `stale`, `projection-lag`, `unavailable`입니다. 읽기 경로는 mutation lease를\n획득하거나 state를 복구하거나 validation gap을 쓰지 않으며 corrupt, unbound,\nSQLite-only, directory-scan-only candidate를 허용하지 않습니다."
    );
    assert!(english.contains("closed 27-row\noutcome table"));
    assert!(english.contains("exact E9 lag marker until repair converges"));
    assert!(korean.contains("closed 27-row outcome table"));
    assert!(korean.contains("exact E9 lag marker를 보존"));
}

#[test]
fn patch_terminal_guard_is_scoped_to_completion_reports() {
    let terminal = "패치 작업 완료\nSummary\n- 결과: 성공".to_string();
    assert_eq!(
        runtime_report::guard_patch_terminal(terminal.clone()),
        crate::runtime_core::reporting::korean_guard::guard_or_failure(&terminal)
    );

    let non_terminal = "patch approve\nSummary\n- status: applied".to_string();
    assert_eq!(
        runtime_report::guard_patch_terminal(non_terminal.clone()),
        non_terminal
    );
}

#[test]
fn doctor_report_field_order_is_stable() {
    let prefixes = doctor_report()
        .lines()
        .map(|line| {
            line.split_once(':')
                .map_or(line, |(prefix, _)| prefix)
                .to_string()
        })
        .collect::<Vec<_>>();

    assert_eq!(
        prefixes,
        [
            "rpotato 진단",
            "- CLI",
            "- package",
            "- package version",
            "- release target os",
            "- release target arch",
            "- release binary suffix",
            "- release smoke",
            "- TUI outcome contract",
            "- runtime core",
            "- backend",
            "- model",
            "- web search",
            "- ontology",
            "- cache",
        ]
    );
}

#[test]
fn doctor_report_includes_release_smoke_fields() {
    let report = doctor_report();

    assert!(report.contains("package: rpotato"));
    assert!(report.contains(&format!("package version: {}", env!("CARGO_PKG_VERSION"))));
    assert!(report.contains("release target os:"));
    assert!(report.contains("release target arch:"));
    assert!(report.contains("release binary suffix:"));
    assert!(report.contains("release smoke: available"));
    assert!(report.contains("web search:"));
}
