use super::*;

mod approval_cases;
mod recovery_cases;
mod support_cases;
mod terminal_cases;
mod verification_cases;

fn report_value(report: &str, key: &str) -> Option<String> {
    let prefix = format!("- {key}: ");
    report
        .lines()
        .find_map(|line| line.strip_prefix(&prefix).map(|value| value.to_string()))
}

fn verification_token(report: &str) -> String {
    report_value(report, "verification command approval")
        .and_then(|command| {
            command
                .split_once(" --token ")
                .map(|(_, token)| token.to_string())
        })
        .expect("verification approval token")
}

fn patch_test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rpotato-patch-{name}-{}", std::process::id()))
}

fn set_patch_test_env(root: &Path) {
    let _ = fs::remove_dir_all(root);
    std::env::set_var("RPOTATO_PROJECT_ROOT", root.join("project"));
    std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));
}

fn clear_patch_test_env(root: &Path) {
    std::env::remove_var("RPOTATO_PROJECT_ROOT");
    std::env::remove_var("RPOTATO_DATA_HOME");
    let _ = fs::remove_dir_all(root);
}

fn create_pending_workflow(
    root: &Path,
    verification: &str,
) -> (PathBuf, state::WorkflowRecord, WorkflowProposal) {
    set_patch_test_env(root);
    let target = root.join("project/src/lib.rs");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "pub const X: i32 = 1;\n").unwrap();
    state::initialize().unwrap();
    let mut workflow = state::create_workflow("change X").unwrap();
    let proposal = prepare_workflow_proposal(
        &workflow.workflow_id,
        &workflow.action_id,
        "src/lib.rs",
        "1",
        "2",
        verification,
    )
    .unwrap();
    workflow.source_path = proposal.relative_path.clone();
    workflow.source_hash = proposal.original_sha256.clone();
    workflow.before_hash = proposal.original_sha256.clone();
    workflow.after_hash = proposal.proposed_sha256.clone();
    workflow.proposal_id = proposal.proposal_id.clone();
    workflow.proposal_hash = proposal.proposal_hash.clone();
    workflow.approval_credential_hash = proposal.approval_credential_hash.clone();
    workflow.verification_plan = proposal.verification_command.clone();
    workflow.approval_state = "pending".to_string();
    workflow.phase = "pending-approval".to_string();
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(state).unwrap();
    }
    for hook in [
        "session_start",
        "user_request_received",
        "pre_context_pack",
        "post_context_pack",
        "pre_model_request",
        "post_model_response",
        "pre_action_parse",
        "post_action_parse",
        "pre_tool_call",
        "post_tool_result",
    ] {
        skill.record_hook(hook).unwrap();
    }
    skill.record_evidence("diff_review");
    skill.store_in_workflow(&mut workflow);
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    (target, workflow, proposal)
}

fn create_prepared_pending_workflow(
    root: &Path,
    verification: &str,
) -> (PathBuf, state::WorkflowRecord, WorkflowProposal) {
    let (target, mut workflow, proposal) = create_pending_workflow(root, verification);
    let mut skill = skill::SkillRuntimeState::new("small-patch", "explicit").unwrap();
    for skill_state in [
        skill::SkillState::ContextReady,
        skill::SkillState::ModelRequested,
        skill::SkillState::ActionRecorded,
        skill::SkillState::AwaitingApproval,
    ] {
        skill.transition(skill_state).unwrap();
    }
    skill.store_in_workflow(&mut workflow);
    state::checkpoint_workflow(workflow.clone(), workflow.revision).unwrap();
    (target, workflow, proposal)
}
