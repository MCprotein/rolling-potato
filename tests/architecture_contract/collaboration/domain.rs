use super::*;

#[test]
fn collaboration_domain_owns_policy_state_and_codecs() {
    let collaboration_mod = source("src/runtime_core/collaboration/mod.rs");
    let owners: &[(&str, &[&str])] = &[
        (
            "src/runtime_core/collaboration/subagent.rs",
            &[
                "mod record;",
                "mod record_validation;",
                "mod types;",
                "pub use types",
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
                "mod admission;",
                "mod dispatch;",
                "mod events;",
                "mod governor;",
                "mod ownership;",
                "mod policy;",
                "mod types;",
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
    assert_team_domain_modules();
    assert_team_state_domain_modules();
    assert_legacy_domain_roots_absent();
}

fn assert_subagent_domain_modules() {
    let domain_path = "src/runtime_core/collaboration/subagent.rs";
    let launch_path = "src/runtime_core/collaboration/subagent/launch.rs";
    let record_path = "src/runtime_core/collaboration/subagent/record.rs";
    let codec_path = "src/runtime_core/collaboration/subagent/record_codec.rs";
    let validation_path = "src/runtime_core/collaboration/subagent/record_validation.rs";
    let types_path = "src/runtime_core/collaboration/subagent/types.rs";
    let domain = source(domain_path);
    let launch = source(launch_path);
    let record = source(record_path);
    let codec = source(codec_path);
    let validation = source(validation_path);
    let types = source(types_path);

    for (path, module) in [
        (launch_path, "mod launch;"),
        (record_path, "mod record;"),
        (codec_path, "mod record_codec;"),
        (validation_path, "mod record_validation;"),
        (types_path, "mod types;"),
    ] {
        assert_file(path);
        assert_registered(&domain, module, "subagent domain");
    }
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
    for responsibility in [
        "pub(crate) fn create_record_at(",
        "pub(crate) fn transition_to_at(",
    ] {
        assert_moved(&record, &domain, responsibility);
    }
    for responsibility in [
        "pub(crate) fn validate_record(",
        "pub(crate) fn immutable_binding_changed(",
        "pub(crate) fn validate_subagent_id(",
        "pub(crate) fn is_sha256(",
    ] {
        assert_moved(&validation, &domain, responsibility);
    }
    for responsibility in [
        "pub enum SubagentRole",
        "pub enum SubagentTool",
        "pub enum SubagentStatus",
        "pub struct ValidatedLaunch",
        "pub struct SubagentRecordV1",
    ] {
        assert_moved(&types, &domain, responsibility);
    }
    assert_line_bound(&domain, 50, domain_path);
    assert_line_bound(&launch, 225, launch_path);
    assert_line_bound(&record, 125, record_path);
    assert_line_bound(&codec, 250, codec_path);
    assert_line_bound(&validation, 225, validation_path);
    assert_line_bound(&types, 225, types_path);
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

fn assert_team_domain_modules() {
    let domain_path = "src/runtime_core/collaboration/team.rs";
    let admission_path = "src/runtime_core/collaboration/team/admission.rs";
    let dispatch_path = "src/runtime_core/collaboration/team/dispatch.rs";
    let events_path = "src/runtime_core/collaboration/team/events.rs";
    let governor_path = "src/runtime_core/collaboration/team/governor.rs";
    let ownership_path = "src/runtime_core/collaboration/team/ownership.rs";
    let policy_path = "src/runtime_core/collaboration/team/policy.rs";
    let types_path = "src/runtime_core/collaboration/team/types.rs";
    let domain = source(domain_path);
    let admission = source(admission_path);
    let dispatch = source(dispatch_path);
    let events = source(events_path);
    let governor = source(governor_path);
    let ownership = source(ownership_path);
    let policy = source(policy_path);
    let types = source(types_path);

    for path in [
        admission_path,
        dispatch_path,
        events_path,
        governor_path,
        ownership_path,
        policy_path,
        types_path,
    ] {
        assert_file(path);
        let child = Path::new(path).file_stem().unwrap().to_str().unwrap();
        assert_registered(&domain, &format!("mod {child};"), "team domain");
    }
    for (owner, owner_source, responsibilities) in [
        (
            admission_path,
            admission.as_str(),
            [
                "fn overall_status(",
                "fn admission_event_type(",
                "fn admission_summary(",
            ]
            .as_slice(),
        ),
        (
            dispatch_path,
            dispatch.as_str(),
            [
                "fn continuation_decision(",
                "fn dispatch_status(",
                "fn dispatch_event_type(",
                "fn dispatch_summary(",
            ]
            .as_slice(),
        ),
        (
            events_path,
            events.as_str(),
            ["fn is_team_runtime_event("].as_slice(),
        ),
        (
            governor_path,
            governor.as_str(),
            [
                "fn pressure_from_status(",
                "fn governor_status(",
                "fn governor_event_type(",
                "fn governor_summary(",
            ]
            .as_slice(),
        ),
        (
            ownership_path,
            ownership.as_str(),
            ["fn evaluate_ownership_gate("].as_slice(),
        ),
        (
            policy_path,
            policy.as_str(),
            [
                "fn policy_write_paths(",
                "fn evaluate_policy_gate(",
                "fn decision_label(",
            ]
            .as_slice(),
        ),
        (
            types_path,
            types.as_str(),
            [
                "struct ContinuationDecision",
                "struct OwnershipGate",
                "struct PolicyGate",
            ]
            .as_slice(),
        ),
    ] {
        for responsibility in responsibilities {
            assert_moved(owner_source, &domain, responsibility);
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
                "team domain owner has concrete reverse dependency: {owner} -> {dependency}"
            );
        }
    }
    for (owner, owner_source, limit) in [
        (domain_path, domain.as_str(), 40),
        (admission_path, admission.as_str(), 100),
        (dispatch_path, dispatch.as_str(), 175),
        (events_path, events.as_str(), 30),
        (governor_path, governor.as_str(), 100),
        (ownership_path, ownership.as_str(), 100),
        (policy_path, policy.as_str(), 75),
        (types_path, types.as_str(), 125),
    ] {
        assert_line_bound(owner_source, limit, owner);
    }
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
