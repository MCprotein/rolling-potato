use crate::foundation::error::AppError;

use super::manifest::ResolvedSkillManifest;

pub fn enforce_resolved_context(
    skill: &ResolvedSkillManifest,
    available: &[&str],
) -> Result<(), AppError> {
    validate_required(
        skill.id(),
        "context",
        skill.context_requirements(),
        &available
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
}

pub fn enforce_resolved_tool(skill: &ResolvedSkillManifest, tool: &str) -> Result<(), AppError> {
    if skill.allowed_tools().contains(&tool) {
        return Ok(());
    }
    Err(AppError::blocked(format!(
        "skill tool policy 차단\n- skill: {}\n- tool: {}\n- allowed: {}",
        skill.id(),
        tool,
        skill.allowed_tools().join(",")
    )))
}

pub(super) fn validate_required(
    skill_id: &str,
    requirement_kind: &str,
    required: &[&str],
    completed: &[String],
) -> Result<(), AppError> {
    let missing = required
        .iter()
        .copied()
        .filter(|required| !completed.iter().any(|item| item == required))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "skill requirement 차단\n- skill: {}\n- requirement: {}\n- missing: {}",
            skill_id,
            requirement_kind,
            missing.join(",")
        )))
    }
}
