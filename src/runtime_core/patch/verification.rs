//! Verification plan and bounded result ownership.

use crate::foundation::error::AppError;
use crate::runtime_core::policy::decision;

const MAX_VERIFICATION_OUTPUT_CHARS: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerificationPlan {
    pub command: String,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerificationResult {
    pub command: String,
    pub exit_code: String,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryAdmission {
    Normal,
    PreparedJournalOnly,
    InconclusiveNeverRerun,
}

impl VerificationResult {
    pub(crate) fn from_output(
        plan: &VerificationPlan,
        exit_code: Option<i32>,
        stdout: &[u8],
        stderr: &[u8],
    ) -> Self {
        Self {
            command: plan.command.clone(),
            exit_code: exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated-by-signal".to_string()),
            stdout: output_excerpt(stdout),
            stderr: output_excerpt(stderr),
        }
    }

    pub(crate) fn spawn_error(plan: &VerificationPlan, error: &str) -> Self {
        Self {
            command: plan.command.clone(),
            exit_code: "spawn-error".to_string(),
            stdout: "(empty)".to_string(),
            stderr: output_text_excerpt(error),
        }
    }

    pub(crate) fn passed(&self) -> bool {
        self.exit_code == "0"
    }
}

pub(crate) fn build_plan(command: &str) -> Result<VerificationPlan, AppError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage(
            "patch approve verification command는 비어 있을 수 없습니다.",
        ));
    }
    let parsed = decision::parse_patch_verification(trimmed)?;
    Ok(VerificationPlan {
        command: parsed.display,
        argv: parsed.argv,
    })
}

pub(crate) fn is_test_plan(plan: &VerificationPlan) -> bool {
    matches!(plan.argv.as_slice(), [cargo, test, ..] if cargo == "cargo" && test == "test")
}

pub(crate) fn recovery_admission(phase: &str) -> RecoveryAdmission {
    match phase {
        "verification-approved" => RecoveryAdmission::PreparedJournalOnly,
        "verification-started" => RecoveryAdmission::InconclusiveNeverRerun,
        _ => RecoveryAdmission::Normal,
    }
}

fn output_excerpt(bytes: &[u8]) -> String {
    output_text_excerpt(&String::from_utf8_lossy(bytes))
}

fn output_text_excerpt(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "(empty)".to_string();
    }
    let mut output = trimmed
        .chars()
        .take(MAX_VERIFICATION_OUTPUT_CHARS)
        .collect::<String>()
        .replace('\n', "\\n");
    if trimmed.chars().count() > MAX_VERIFICATION_OUTPUT_CHARS {
        output.push_str("...");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_is_policy_parsed_and_test_detection_is_exact() {
        let plan = build_plan("cargo test --locked").unwrap();
        assert_eq!(plan.argv, ["cargo", "test", "--locked"]);
        assert!(is_test_plan(&plan));

        let read_only = build_plan("pwd").unwrap();
        assert!(!is_test_plan(&read_only));
    }

    #[test]
    fn result_output_is_bounded_and_status_is_truthful() {
        let plan = build_plan("pwd").unwrap();
        let output = "가".repeat(MAX_VERIFICATION_OUTPUT_CHARS + 1);
        let result = VerificationResult::from_output(&plan, Some(1), output.as_bytes(), b"");

        assert_eq!(result.exit_code, "1");
        assert!(!result.passed());
        assert_eq!(
            result.stdout.chars().count(),
            MAX_VERIFICATION_OUTPUT_CHARS + 3
        );
        assert!(result.stdout.ends_with("..."));
        assert_eq!(result.stderr, "(empty)");
    }

    #[test]
    fn started_verification_never_auto_reruns_during_recovery() {
        assert_eq!(
            recovery_admission("verification-started"),
            RecoveryAdmission::InconclusiveNeverRerun
        );
        assert_eq!(
            recovery_admission("verification-approved"),
            RecoveryAdmission::PreparedJournalOnly
        );
    }
}
