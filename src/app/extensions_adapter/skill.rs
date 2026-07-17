//! Concrete skill adapters for plugin discovery and workflow persistence.

use crate::foundation::error::AppError;
use crate::state;

pub(crate) use crate::runtime_core::extensions::skill::*;

impl SkillRuntimeState {
    #[cfg(test)]
    pub fn new(skill_id: &str, invocation: &str) -> Result<Self, AppError> {
        let manifest = resolve_skill(skill_id)?.ok_or_else(|| {
            AppError::usage(format!("등록된 skill을 찾지 못했습니다: {skill_id}"))
        })?;
        Self::new_resolved(&manifest, invocation)
    }

    pub fn validate_stop(&self) -> Result<(), AppError> {
        let manifest = resolve_skill(&self.active_skill_id)?.ok_or_else(|| {
            AppError::blocked(format!(
                "skill stop gate 차단\n- skill: {}\n- 이유: manifest 없음",
                self.active_skill_id
            ))
        })?;
        self.validate_stop_against(&manifest)
    }

    pub fn from_workflow(workflow: &state::WorkflowRecord) -> Result<Self, AppError> {
        let manifest = resolve_skill(&workflow.active_skill_id)?.ok_or_else(|| {
            AppError::blocked(format!(
                "skill resume 차단\n- workflow: {}\n- 이유: skill manifest 없음: {}",
                workflow.workflow_id, workflow.active_skill_id
            ))
        })?;
        Self::from_workflow_against(workflow, &manifest)
    }

    pub fn from_workflow_against(
        workflow: &state::WorkflowRecord,
        manifest: &ResolvedSkillManifest,
    ) -> Result<Self, AppError> {
        if workflow.active_skill_id != manifest.id() {
            return Err(AppError::blocked(format!(
                "skill resume 차단\n- workflow: {}\n- stored skill: {}\n- resolved skill: {}",
                workflow.workflow_id,
                workflow.active_skill_id,
                manifest.id()
            )));
        }
        let state = SkillState::parse(&workflow.skill_state).ok_or_else(|| {
            AppError::blocked(format!(
                "skill resume 차단\n- workflow: {}\n- skill: {}\n- state: {}",
                workflow.workflow_id, workflow.active_skill_id, workflow.skill_state
            ))
        })?;
        let runtime = Self {
            active_skill_id: workflow.active_skill_id.clone(),
            invocation: workflow.skill_invocation.clone(),
            state,
            completed_hooks: split_labels(&workflow.skill_completed_hooks),
            evidence: split_labels(&workflow.skill_evidence),
            completed_stop_criteria: split_labels(&workflow.skill_stop_criteria),
        };
        if !matches!(runtime.invocation.as_str(), "explicit" | "natural-language") {
            return Err(AppError::blocked(format!(
                "skill resume 차단\n- workflow: {}\n- 이유: 알 수 없는 invocation source: {}",
                workflow.workflow_id, runtime.invocation
            )));
        }
        Ok(runtime)
    }

    pub fn store_in_workflow(&self, workflow: &mut state::WorkflowRecord) {
        workflow.active_skill_id = self.active_skill_id.clone();
        workflow.skill_invocation = self.invocation.clone();
        workflow.skill_state = self.state.label().to_string();
        workflow.skill_completed_hooks = self.completed_hooks.join(",");
        workflow.skill_evidence = self.evidence.join(",");
        workflow.skill_stop_criteria = self.completed_stop_criteria.join(",");
    }
}

pub fn list_report() -> String {
    let mut skills = BUILTIN_SKILLS
        .iter()
        .map(|skill| {
            format!(
                "- {} ({}) | mode: {} | {}",
                skill.id, skill.display_name, skill.mode, skill.description
            )
        })
        .collect::<Vec<_>>();
    let imported = crate::plugin::enabled_codex_skill_rows();
    skills.extend(imported.iter().cloned());

    format!(
        "skill registry\n- native skills: {}\n- enabled imported Codex skills: {}\n- imported skill namespace: imported.codex.<plugin>.<skill>\n- 실행 경계: imported skill은 실행 시 source snapshot과 SKILL.md를 다시 검증하고 runtime policy/evidence gate를 통과해야 합니다.\n{}",
        BUILTIN_SKILLS.len(),
        imported.len(),
        skills.join("\n")
    )
}

pub fn resolve_skill(id: &str) -> Result<Option<ResolvedSkillManifest>, AppError> {
    if let Some(manifest) = find_skill(id) {
        return Ok(Some(ResolvedSkillManifest::Builtin(manifest)));
    }
    crate::plugin::resolve_imported_codex_skill(id)
        .map(|manifest| manifest.map(ResolvedSkillManifest::Imported))
}

fn split_labels(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_builtin_skill() {
        let skill = find_skill("fix-test").unwrap();
        assert_eq!(skill.mode, "execute");
        assert!(skill.evidence_requirements.contains(&"passing_test_after"));
    }

    #[test]
    fn list_includes_import_namespace_rule() {
        let report = list_report();
        assert!(report.contains("imported.codex.<plugin>.<skill>"));
    }

    #[test]
    fn explicit_and_natural_invocations_start_selected() {
        for invocation in ["explicit", "natural-language"] {
            let runtime = SkillRuntimeState::new("small-patch", invocation).unwrap();
            assert_eq!(runtime.state, SkillState::Selected);
            assert_eq!(runtime.invocation, invocation);
        }
    }

    #[test]
    fn state_machine_rejects_skipped_gate() {
        let mut runtime = SkillRuntimeState::new("small-patch", "explicit").unwrap();
        let error = runtime.transition(SkillState::ActionRecorded).unwrap_err();

        assert_eq!(error.code, 3);
        assert_eq!(runtime.state, SkillState::Selected);
    }

    #[test]
    fn missing_context_fails_closed() {
        let skill = ResolvedSkillManifest::Builtin(find_skill("small-patch").unwrap());
        let error = enforce_resolved_context(&skill, &["target_file"]).unwrap_err();

        assert!(error.message.contains("acceptance_criteria"));
    }

    #[test]
    fn tool_outside_manifest_is_denied() {
        let skill = ResolvedSkillManifest::Builtin(find_skill("model-artifact-audit").unwrap());
        let error = enforce_resolved_tool(&skill, "run_command").unwrap_err();

        assert!(error.message.contains("tool policy 차단"));
    }

    #[test]
    fn stop_gate_requires_hooks_evidence_and_criteria() {
        let mut runtime = SkillRuntimeState::new("repo-map", "natural-language").unwrap();
        for hook in READ_ONLY_HOOKS {
            runtime.record_hook(hook).unwrap();
        }
        runtime.record_stop_criterion("map_reported");
        runtime.record_stop_criterion("korean_report_passed");

        let error = runtime.validate_stop().unwrap_err();
        assert!(error.message.contains("file_reference"));

        runtime.record_evidence("file_reference");
        runtime.validate_stop().unwrap();
    }

    #[test]
    fn execute_skills_require_all_lifecycle_hooks() {
        let skill = find_skill("fix-test").unwrap();

        assert_eq!(skill.required_hooks, EXECUTE_HOOKS);
        assert!(skill.required_hooks.contains(&"pre_patch_apply"));
        assert!(skill.required_hooks.contains(&"stop_gate"));
    }

    #[test]
    fn runtime_state_round_trips_through_workflow_fields() {
        let identity = crate::ledger::fresh_identity();
        let mut workflow = state::WorkflowRecord::new(&identity, "request");
        let mut runtime = SkillRuntimeState::new("small-patch", "explicit").unwrap();
        runtime.transition(SkillState::ContextReady).unwrap();
        runtime.record_hook("session_start").unwrap();
        runtime.record_evidence("diff_review");
        runtime.record_stop_criterion("patch_applied");

        runtime.store_in_workflow(&mut workflow);
        let restored = SkillRuntimeState::from_workflow(&workflow).unwrap();

        assert_eq!(restored, runtime);
    }
}
