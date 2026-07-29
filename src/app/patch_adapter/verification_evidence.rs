use super::*;

pub fn validate_skill_verification(skill_id: &str, command: &str) -> Result<(), AppError> {
    let plan = build_verification_plan(command)?;
    if skill_id == "fix-test" && !verification_domain::is_test_plan(&plan) {
        return Err(AppError::blocked(
            "fix-test verification 차단\n- 이유: fix-test는 실제 `cargo test` command로만 전후 evidence를 만들 수 있습니다.",
        ));
    }
    Ok(())
}

pub fn record_failing_test_before(
    workflow: &state::WorkflowRecord,
    command: &str,
) -> Result<String, AppError> {
    validate_skill_verification("fix-test", command)?;
    let plan = build_verification_plan(command)?;
    let result = run_verification(&plan);
    let failed_exit = result
        .exit_code
        .parse::<i32>()
        .ok()
        .is_some_and(|code| code != 0);
    if !failed_exit {
        return Err(AppError::blocked(format!(
            "fix-test 시작 차단\n- 이유: patch 전 실제 test failure를 관측하지 못했습니다.\n- exit code: {}\n- command: {}",
            result.exit_code,
            ledger::redact_text(&result.command)
        )));
    }
    state::record_event(
        "skill.test_failure.observed",
        "fix-test patch 전 실패 관측",
        &format!(
            "workflow_id={} command_hash={} exit_code={} stdout_hash={} stderr_hash={}",
            workflow.workflow_id,
            state::sha256_text(&plan.command),
            result.exit_code,
            state::sha256_text(&result.stdout),
            state::sha256_text(&result.stderr)
        ),
    )
}
