use super::*;

#[test]
fn collaboration_domain_owns_policy_state_and_codecs() {
    let collaboration_mod = source("src/runtime_core/collaboration/mod.rs");
    let owners: &[(&str, &[&str])] = &[
        (
            "src/runtime_core/collaboration/subagent.rs",
            &[
                "enum SubagentRole",
                "enum SubagentStatus",
                "struct SubagentRecordV1",
                "fn validate_record",
            ],
        ),
        (
            "src/runtime_core/collaboration/subagent_result.rs",
            &[
                "struct SubagentResultV1",
                "fn parse_result_shape",
                "fn validate_patch_policy",
                "fn validate_context_binding",
                "fn validate_bounded_text",
            ],
        ),
        (
            "src/runtime_core/collaboration/team.rs",
            &[
                "struct ContinuationDecision",
                "struct PolicyGate",
                "fn continuation_decision",
                "fn evaluate_policy_gate",
                "fn evaluate_ownership_gate",
                "fn dispatch_event_type",
                "fn admission_summary",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_execution.rs",
            &[
                "fn validate_execution_binding",
                "fn validate_execution_stage",
                "fn execution_mode",
                "fn validate_action_owner",
                "fn record_matches_team",
                "fn validate_completed_member_binding",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_reconciliation.rs",
            &[
                "fn validate_reconciliation_binding",
                "fn validate_reconciliation_stage",
                "fn validate_action_ownership",
                "fn validate_member_record",
                "fn render_reconciliation",
            ],
        ),
        (
            "src/runtime_core/collaboration/team_state.rs",
            &[
                "enum TeamStage",
                "impl TeamStage",
                "struct TeamStateV1",
                "fn transition_to_at",
            ],
        ),
    ];

    for (owner, rules) in owners {
        assert_file(owner);
        let child = Path::new(owner).file_stem().unwrap().to_str().unwrap();
        assert_registered(
            &collaboration_mod,
            &format!("pub(crate) mod {child};"),
            "collaboration domain",
        );
        let owner_source = source(owner);
        for rule in *rules {
            assert!(
                owner_source.contains(rule),
                "collaboration domain is missing rule: {owner} -> {rule}"
            );
        }
        for dependency in [
            "crate::adapters",
            "crate::backend",
            "crate::ledger",
            "crate::observability",
            "crate::state",
            "std::fs",
            "std::process",
            "std::thread",
        ] {
            assert!(
                !owner_source.contains(dependency),
                "collaboration domain has concrete reverse dependency: {owner} -> {dependency}"
            );
        }
    }

    assert_subagent_domain_modules();
    assert_subagent_result_modules();
    assert_team_state_domain_modules();
    assert_legacy_domain_roots_absent();
}

fn assert_subagent_domain_modules() {
    let domain_path = "src/runtime_core/collaboration/subagent.rs";
    let launch_path = "src/runtime_core/collaboration/subagent/launch.rs";
    let codec_path = "src/runtime_core/collaboration/subagent/record_codec.rs";
    let domain = source(domain_path);
    let launch = source(launch_path);
    let codec = source(codec_path);

    assert_file(launch_path);
    assert_file(codec_path);
    assert_registered(&domain, "mod launch;", "subagent domain");
    assert_registered(&domain, "mod record_codec;", "subagent domain");
    for responsibility in [
        "pub fn validate_launch(",
        "pub(crate) fn normalize_tools(",
        "pub(crate) fn normalize_paths(",
        "pub(crate) fn normalize_relative_path(",
    ] {
        assert_moved(&launch, &domain, responsibility);
    }
    for responsibility in [
        "pub(crate) fn render_payload(",
        "pub(crate) fn render_record(",
        "pub(crate) fn parse_record(",
        "fn canonical_string(",
        "fn canonical_string_array(",
    ] {
        assert_moved(&codec, &domain, responsibility);
    }
    assert_line_bound(&domain, 450, domain_path);
    assert_line_bound(&launch, 225, launch_path);
    assert_line_bound(&codec, 250, codec_path);
}

fn assert_subagent_result_modules() {
    let domain_path = "src/runtime_core/collaboration/subagent_result.rs";
    let evidence_path = "src/runtime_core/collaboration/subagent_result/evidence.rs";
    let domain = source(domain_path);
    let evidence = source(evidence_path);

    assert_file(evidence_path);
    assert_registered(&domain, "mod evidence;", "subagent result domain");
    for responsibility in [
        "const EVIDENCE_V2_KEYS",
        "pub(crate) fn evidence_source_bindings(",
        "pub(crate) fn verify_evidence_artifact(",
        "pub(crate) fn render_evidence_payload_v2(",
        "pub(crate) fn evidence_id(",
        "pub(crate) fn installable_evidence_body(",
    ] {
        assert_moved(&evidence, &domain, responsibility);
    }
    assert_line_bound(&domain, 350, domain_path);
    assert_line_bound(&evidence, 300, evidence_path);
}

fn assert_team_state_domain_modules() {
    let domain_path = "src/runtime_core/collaboration/team_state.rs";
    let manifest_path = "src/runtime_core/collaboration/team_state/manifest_codec.rs";
    let state_path = "src/runtime_core/collaboration/team_state/state_codec.rs";
    let validation_path = "src/runtime_core/collaboration/team_state/validation.rs";
    let domain = source(domain_path);
    let manifest = source(manifest_path);
    let state = source(state_path);
    let validation = source(validation_path);

    for path in [manifest_path, state_path, validation_path] {
        assert_file(path);
    }
    for module in ["mod manifest_codec;", "mod state_codec;", "mod validation;"] {
        assert_registered(&domain, module, "team state domain");
    }
    for responsibility in [
        "pub fn parse_manifest(",
        "fn parse_members(",
        "fn validate_member_set(",
    ] {
        assert_moved(&manifest, &domain, responsibility);
    }
    for responsibility in [
        "pub(crate) fn render_payload(",
        "pub(crate) fn render_state(",
        "pub(crate) fn parse_state(",
        "pub(crate) fn validate_state(",
        "pub(crate) fn immutable_binding_changed(",
    ] {
        assert_moved(&state, &domain, responsibility);
    }
    for responsibility in ["pub(crate) fn validate_id(", "pub(crate) fn is_sha256("] {
        assert_moved(&validation, &domain, responsibility);
    }
    for (owner, owner_source, limit) in [
        (domain_path, domain.as_str(), 225),
        (manifest_path, manifest.as_str(), 250),
        (state_path, state.as_str(), 225),
        (validation_path, validation.as_str(), 50),
    ] {
        assert_line_bound(owner_source, limit, owner);
    }
}

fn assert_legacy_domain_roots_absent() {
    for legacy in [
        "src/subagent.rs",
        "src/team.rs",
        "src/team_execution.rs",
        "src/team_reconciliation.rs",
        "src/team_state.rs",
        "src/subagent_result.rs",
    ] {
        assert!(
            !Path::new(legacy).exists(),
            "legacy collaboration root was restored: {legacy}"
        );
    }

    let main = source("src/main.rs");
    for legacy_mod in [
        "mod subagent;",
        "mod subagent_result;",
        "mod team;",
        "mod team_execution;",
        "mod team_reconciliation;",
        "mod team_state;",
        "pub mod team_state;",
    ] {
        assert!(
            !main.lines().any(|line| line == legacy_mod),
            "legacy collaboration root remains registered: {legacy_mod}"
        );
    }
}
