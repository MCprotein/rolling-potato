#[test]
fn v0378_knowledge_and_policy_owners_hold_domain_rules() {
    assert!(Path::new("src/app/ontology_adapter.rs").is_file());
    assert!(Path::new("src/app/ontology_adapter/seeding.rs").is_file());
    assert!(!Path::new("src/ontology.rs").exists());
    assert!(!Path::new("src/ontology").exists());
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod ontology;"));
    let app_root = fs::read_to_string("src/app.rs").unwrap();
    assert!(
        app_root
            .lines()
            .any(|line| line == "pub(crate) mod ontology_adapter;"),
        "application root does not register the ontology adapter"
    );
    let owners = [
        "src/runtime_core/knowledge/compaction.rs",
        "src/runtime_core/knowledge/context.rs",
        "src/runtime_core/knowledge/evidence.rs",
        "src/runtime_core/knowledge/ontology.rs",
        "src/runtime_core/policy/approval.rs",
        "src/runtime_core/policy/decision.rs",
        "src/runtime_core/policy/redaction.rs",
    ];
    for target in owners {
        assert!(
            Path::new(target).is_file(),
            "missing v0.37.8 knowledge/policy owner: {target}"
        );
    }

    let runtime_core = fs::read_to_string("src/runtime_core/mod.rs").unwrap();
    for owner in ["knowledge", "policy"] {
        let expected = format!("pub(crate) mod {owner};");
        assert!(
            runtime_core.lines().any(|line| line == expected),
            "runtime owner is not crate-private: {owner}"
        );
    }
    for (module, children) in [
        (
            "src/runtime_core/knowledge/mod.rs",
            [
                "compaction",
                "context",
                "evidence",
                "ontology",
                "prompt",
                "recall",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/policy/mod.rs",
            ["approval", "decision", "redaction"].as_slice(),
        ),
    ] {
        let source = fs::read_to_string(module).unwrap();
        for child in children {
            let expected = format!("pub(crate) mod {child};");
            assert!(
                source.lines().any(|line| line == expected),
                "runtime child is not crate-private: {module} -> {child}"
            );
        }
    }

    let compaction = fs::read_to_string("src/runtime_core/knowledge/compaction.rs").unwrap();
    for child in ["checkpoint", "policy", "token_budget"] {
        assert!(
            compaction
                .lines()
                .any(|line| line == format!("mod {child};")),
            "compaction facade does not register its {child} owner"
        );
    }

    let context = fs::read_to_string("src/runtime_core/knowledge/context.rs").unwrap();
    for owner in ["assembly", "budget", "resume", "sources", "types"] {
        assert!(
            context.contains(&format!("#[path = \"context/{owner}.rs\"]")),
            "context facade does not register its {owner} owner"
        );
    }

    for (owner, rules) in [
        (
            "src/runtime_core/knowledge/compaction/checkpoint.rs",
            ["struct CompactionCheckpoint"].as_slice(),
        ),
        (
            "src/runtime_core/knowledge/compaction/policy.rs",
            ["struct CompactionPolicy", "fn bounded_summary_source"].as_slice(),
        ),
        (
            "src/runtime_core/knowledge/compaction/token_budget.rs",
            ["fn estimate_tokens"].as_slice(),
        ),
        (
            "src/runtime_core/knowledge/context/types.rs",
            ["struct ContextPack", "struct ResumeContext"].as_slice(),
        ),
        (
            "src/runtime_core/knowledge/context/budget.rs",
            ["struct ResumeContextBudget", "struct AgentPromptBudget"].as_slice(),
        ),
        (
            "src/runtime_core/knowledge/context/sources.rs",
            ["fn enforce_shared_source_budget", "fn truncate_chars"].as_slice(),
        ),
        (
            "src/runtime_core/knowledge/evidence.rs",
            [
                "struct StopGateInputs",
                "fn validate_stop_inputs",
                "fn validate_artifact_pointer_syntax",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/knowledge/ontology.rs",
            ["struct OntologyRecord", "mod projection", "mod selection"].as_slice(),
        ),
        (
            "src/runtime_core/policy/approval.rs",
            [
                "struct ApprovalRequest",
                "fn render_request_record",
                "fn validate_request_id",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/policy/decision.rs",
            [
                "enum Decision",
                "trait PathPolicyPort",
                "fn classify_command",
                "fn classify_path",
            ]
            .as_slice(),
        ),
        (
            "src/runtime_core/policy/redaction.rs",
            ["fn contains_sensitive_text", "fn redact_text"].as_slice(),
        ),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        for rule in rules {
            assert!(
                source.contains(rule),
                "v0.37.8 owner is missing domain rule: {owner} -> {rule}"
            );
        }
        for forbidden in ["crate::adapters", "crate::ledger", "crate::state"] {
            assert!(
                !source.contains(forbidden),
                "runtime knowledge/policy owner has concrete reverse dependency: {owner} -> {forbidden}"
            );
        }
    }

    for (owner, line_budget) in [
        ("src/runtime_core/knowledge/context.rs", 50),
        ("src/runtime_core/knowledge/context/assembly.rs", 125),
        ("src/runtime_core/knowledge/context/budget.rs", 100),
        ("src/runtime_core/knowledge/context/resume.rs", 100),
        ("src/runtime_core/knowledge/context/sources.rs", 150),
        ("src/runtime_core/knowledge/context/types.rs", 75),
    ] {
        let source = fs::read_to_string(owner).unwrap();
        assert!(
            source.lines().count() < line_budget,
            "context owner {owner} exceeded its {line_budget}-line budget"
        );
    }

    let policy_facade = fs::read_to_string("src/app/policy_adapter.rs").unwrap();
    assert!(
        policy_facade.contains("impl PathPolicyPort for ProjectPathPolicy"),
        "filesystem path policy is not composed through the consumer-owned port"
    );

    let ledger_facade = fs::read_to_string("src/app/workflow_adapter/ledger.rs").unwrap();
    assert!(
        ledger_facade.contains(
            "pub use crate::runtime_core::policy::redaction::{contains_sensitive_text, redact_text};"
        ),
        "ledger facade does not preserve the redaction API path"
    );
    for moved_rule in ["pub fn contains_sensitive_text", "pub fn redact_text"] {
        assert!(
            !ledger_facade.contains(moved_rule),
            "ledger facade still owns policy redaction rule: {moved_rule}"
        );
    }

    for (facade, forbidden) in [
        ("src/app/approval_adapter.rs", "struct ApprovalRequest"),
        ("src/app/approval_adapter.rs", "fn render_request_record"),
        ("src/app/context_adapter.rs", "pub struct ContextPack"),
        ("src/app/context_adapter.rs", "fn clamp_source_pack"),
        ("src/app/evidence_adapter.rs", "struct StopGateInputs"),
        ("src/app/evidence_adapter.rs", "fn stale_policy_summary"),
        ("src/app/ontology_adapter.rs", "struct OntologyRecord"),
        ("src/app/ontology_adapter.rs", "fn select_context_records"),
        ("src/app/policy_adapter.rs", "pub enum Decision"),
        (
            "src/app/policy_adapter.rs",
            "fn validate_patch_verification_argv",
        ),
    ] {
        let source = fs::read_to_string(facade).unwrap();
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        assert!(
            !production.contains(forbidden),
            "legacy facade retains moved knowledge/policy rule: {facade} -> {forbidden}"
        );
    }
    assert!(!Path::new("src/approval.rs").exists());
    let approval_adapter = fs::read_to_string("src/app/approval_adapter.rs").unwrap();
    assert!(approval_adapter.contains("pub fn write_request"));
    let main = fs::read_to_string("src/main.rs").unwrap();
    assert!(!main.lines().any(|line| line == "mod approval;"));
    assert!(!Path::new("src/policy.rs").exists());
    assert!(policy_facade.contains("impl PathPolicyPort for ProjectPathPolicy"));
    assert!(!main.lines().any(|line| line == "mod policy;"));
}
