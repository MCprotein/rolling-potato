use crate::app::AppError;
use crate::state;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillManifest {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub mode: &'static str,
    pub required_hooks: &'static [&'static str],
    pub allowed_tools: &'static [&'static str],
    pub context_requirements: &'static [&'static str],
    pub evidence_requirements: &'static [&'static str],
    pub stop_criteria: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedSkillManifest {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub instructions: String,
    pub plugin_id: String,
    pub source_path: String,
    pub source_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSkillManifest {
    Builtin(&'static SkillManifest),
    Imported(ImportedSkillManifest),
}

const IMPORTED_SKILL_TOOLS: &[&str] = &["read_file"];
const IMPORTED_SKILL_CONTEXT: &[&str] = &["repo_root"];
const IMPORTED_SKILL_EVIDENCE: &[&str] = &["plugin_capability_admission"];
const IMPORTED_SKILL_STOP: &[&str] = &["plugin_capability_completed", "korean_report_passed"];

impl ResolvedSkillManifest {
    pub fn id(&self) -> &str {
        match self {
            Self::Builtin(manifest) => manifest.id,
            Self::Imported(manifest) => &manifest.id,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::Builtin(manifest) => manifest.display_name,
            Self::Imported(manifest) => &manifest.display_name,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Builtin(manifest) => manifest.description,
            Self::Imported(manifest) => &manifest.description,
        }
    }

    pub fn mode(&self) -> &'static str {
        match self {
            Self::Builtin(manifest) => manifest.mode,
            Self::Imported(_) => "read-only",
        }
    }

    pub fn required_hooks(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.required_hooks,
            Self::Imported(_) => READ_ONLY_HOOKS,
        }
    }

    pub fn allowed_tools(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.allowed_tools,
            Self::Imported(_) => IMPORTED_SKILL_TOOLS,
        }
    }

    pub fn context_requirements(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.context_requirements,
            Self::Imported(_) => IMPORTED_SKILL_CONTEXT,
        }
    }

    pub fn evidence_requirements(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.evidence_requirements,
            Self::Imported(_) => IMPORTED_SKILL_EVIDENCE,
        }
    }

    pub fn stop_criteria(&self) -> &'static [&'static str] {
        match self {
            Self::Builtin(manifest) => manifest.stop_criteria,
            Self::Imported(_) => IMPORTED_SKILL_STOP,
        }
    }

    pub fn instructions(&self) -> &str {
        match self {
            Self::Builtin(manifest) => manifest.description,
            Self::Imported(manifest) => &manifest.instructions,
        }
    }

    pub fn imported(&self) -> Option<&ImportedSkillManifest> {
        match self {
            Self::Builtin(_) => None,
            Self::Imported(manifest) => Some(manifest),
        }
    }
}

pub const READ_ONLY_HOOKS: &[&str] = &[
    "session_start",
    "user_request_received",
    "pre_context_pack",
    "post_context_pack",
    "pre_model_request",
    "post_model_response",
    "pre_action_parse",
    "post_action_parse",
    "pre_final_report",
    "stop_gate",
    "session_end",
];

pub const EXECUTE_HOOKS: &[&str] = &[
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
    "pre_patch_apply",
    "post_patch_apply",
    "pre_command_run",
    "post_command_run",
    "pre_final_report",
    "stop_gate",
    "session_end",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillState {
    Selected,
    ContextReady,
    ModelRequested,
    ActionRecorded,
    AwaitingApproval,
    AwaitingVerification,
    StopPassed,
    Complete,
    Failed,
    Cancelled,
}

impl SkillState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Selected => "selected",
            Self::ContextReady => "context-ready",
            Self::ModelRequested => "model-requested",
            Self::ActionRecorded => "action-recorded",
            Self::AwaitingApproval => "awaiting-approval",
            Self::AwaitingVerification => "awaiting-verification",
            Self::StopPassed => "stop-passed",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "selected" => Self::Selected,
            "context-ready" => Self::ContextReady,
            "model-requested" => Self::ModelRequested,
            "action-recorded" => Self::ActionRecorded,
            "awaiting-approval" => Self::AwaitingApproval,
            "awaiting-verification" => Self::AwaitingVerification,
            "stop-passed" => Self::StopPassed,
            "complete" => Self::Complete,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRuntimeState {
    pub active_skill_id: String,
    pub invocation: String,
    pub state: SkillState,
    pub completed_hooks: Vec<String>,
    pub evidence: Vec<String>,
    pub completed_stop_criteria: Vec<String>,
}

impl SkillRuntimeState {
    pub fn new(skill_id: &str, invocation: &str) -> Result<Self, AppError> {
        if resolve_skill(skill_id)?.is_none() {
            return Err(AppError::usage(format!(
                "등록된 skill을 찾지 못했습니다: {skill_id}"
            )));
        }
        if !matches!(invocation, "explicit" | "natural-language") {
            return Err(AppError::blocked(format!(
                "skill invocation 차단\n- skill: {skill_id}\n- 이유: 알 수 없는 invocation source: {invocation}"
            )));
        }
        Ok(Self {
            active_skill_id: skill_id.to_string(),
            invocation: invocation.to_string(),
            state: SkillState::Selected,
            completed_hooks: Vec::new(),
            evidence: Vec::new(),
            completed_stop_criteria: Vec::new(),
        })
    }

    pub fn transition(&mut self, next: SkillState) -> Result<(), AppError> {
        validate_transition(self.state, next)?;
        self.state = next;
        Ok(())
    }

    pub fn record_hook(&mut self, hook: &str) -> Result<(), AppError> {
        if !crate::hooks::HOOK_POINTS
            .iter()
            .any(|point| point.name == hook)
        {
            return Err(AppError::blocked(format!(
                "skill hook 기록 차단\n- skill: {}\n- hook: {}\n- 이유: 등록되지 않은 hook point",
                self.active_skill_id, hook
            )));
        }
        push_unique(&mut self.completed_hooks, hook);
        Ok(())
    }

    pub fn record_evidence(&mut self, evidence: &str) {
        push_unique(&mut self.evidence, evidence);
    }

    pub fn record_stop_criterion(&mut self, criterion: &str) {
        push_unique(&mut self.completed_stop_criteria, criterion);
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

    pub fn validate_stop_against(&self, manifest: &ResolvedSkillManifest) -> Result<(), AppError> {
        validate_required(
            manifest.id(),
            "hook",
            manifest.required_hooks(),
            &self.completed_hooks,
        )?;
        validate_required(
            manifest.id(),
            "evidence",
            manifest.evidence_requirements(),
            &self.evidence,
        )?;
        validate_required(
            manifest.id(),
            "stop criterion",
            manifest.stop_criteria(),
            &self.completed_stop_criteria,
        )
    }

    pub fn from_workflow(workflow: &state::WorkflowRecord) -> Result<Self, AppError> {
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
        if resolve_skill(&runtime.active_skill_id)?.is_none() {
            return Err(AppError::blocked(format!(
                "skill resume 차단\n- workflow: {}\n- 이유: skill manifest 없음: {}",
                workflow.workflow_id, runtime.active_skill_id
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

pub const BUILTIN_SKILLS: &[SkillManifest] = &[
    SkillManifest {
        id: "fix-test",
        display_name: "Fix Test",
        description: "실패한 테스트 하나를 좁은 범위에서 수정하고 검증한다.",
        mode: "execute",
        required_hooks: EXECUTE_HOOKS,
        allowed_tools: &["read_file", "render_diff", "apply_patch", "run_command"],
        context_requirements: &["test_output", "source_pointer", "package_manifest"],
        evidence_requirements: &["failing_test_before", "passing_test_after"],
        stop_criteria: &[
            "patch_applied",
            "verification_passed",
            "korean_report_passed",
        ],
    },
    SkillManifest {
        id: "explain-error",
        display_name: "Explain Error",
        description: "오류 원인을 코드와 로그 근거로 설명한다.",
        mode: "read-only",
        required_hooks: READ_ONLY_HOOKS,
        allowed_tools: &["read_file", "run_read_only_command"],
        context_requirements: &["error_output", "source_pointer"],
        evidence_requirements: &["source_reference"],
        stop_criteria: &["cause_explained", "korean_report_passed"],
    },
    SkillManifest {
        id: "small-patch",
        display_name: "Small Patch",
        description: "작고 되돌릴 수 있는 패치 하나를 제안한다.",
        mode: "execute",
        required_hooks: EXECUTE_HOOKS,
        allowed_tools: &["read_file", "render_diff", "apply_patch", "run_command"],
        context_requirements: &["target_file", "acceptance_criteria"],
        evidence_requirements: &["diff_review", "targeted_verification"],
        stop_criteria: &[
            "patch_applied",
            "verification_passed",
            "korean_report_passed",
        ],
    },
    SkillManifest {
        id: "code-review",
        display_name: "Code Review",
        description: "버그, 회귀, 테스트 누락을 우선으로 리뷰한다.",
        mode: "review-only",
        required_hooks: READ_ONLY_HOOKS,
        allowed_tools: &["read_file", "run_read_only_command"],
        context_requirements: &["diff_or_files", "test_context"],
        evidence_requirements: &["file_line_reference"],
        stop_criteria: &["findings_ranked", "korean_report_passed"],
    },
    SkillManifest {
        id: "repo-map",
        display_name: "Repo Map",
        description: "저장소 구조와 관련 파일을 읽기 전용으로 매핑한다.",
        mode: "read-only",
        required_hooks: READ_ONLY_HOOKS,
        allowed_tools: &["read_file", "run_read_only_command"],
        context_requirements: &["repo_root"],
        evidence_requirements: &["file_reference"],
        stop_criteria: &["map_reported", "korean_report_passed"],
    },
    SkillManifest {
        id: "benchmark-model",
        display_name: "Benchmark Model",
        description: "모델 후보를 출처 기반 benchmark 계획으로 평가한다.",
        mode: "plan-only",
        required_hooks: READ_ONLY_HOOKS,
        allowed_tools: &["read_file", "run_read_only_command"],
        context_requirements: &["model_manifest", "benchmark_spec"],
        evidence_requirements: &["benchmark_source", "local_result_artifact"],
        stop_criteria: &["benchmark_plan_ready", "korean_report_passed"],
    },
    SkillManifest {
        id: "model-artifact-audit",
        display_name: "Model Artifact Audit",
        description: "모델 artifact 출처, 라이선스, checksum을 검토한다.",
        mode: "read-only",
        required_hooks: READ_ONLY_HOOKS,
        allowed_tools: &["read_file"],
        context_requirements: &["model_source", "license_source", "artifact_manifest"],
        evidence_requirements: &["source_url_or_file", "checksum_record"],
        stop_criteria: &["claims_source_backed", "korean_report_passed"],
    },
    SkillManifest {
        id: "runtime-doctor",
        display_name: "Runtime Doctor",
        description: "runtime state/backend/cache 상태를 진단한다.",
        mode: "read-only",
        required_hooks: READ_ONLY_HOOKS,
        allowed_tools: &["read_file", "run_read_only_command"],
        context_requirements: &["runtime_state", "operation_log"],
        evidence_requirements: &["diagnostic_output"],
        stop_criteria: &["diagnosis_reported", "korean_report_passed"],
    },
    SkillManifest {
        id: "ontology-refresh",
        display_name: "Ontology Refresh",
        description: "프로젝트 ontology pointer와 source evidence를 갱신한다.",
        mode: "plan-only",
        required_hooks: READ_ONLY_HOOKS,
        allowed_tools: &["read_file"],
        context_requirements: &["ontology_source", "source_pointer"],
        evidence_requirements: &["source_reference", "confidence_record"],
        stop_criteria: &["ontology_delta_ready", "korean_report_passed"],
    },
    SkillManifest {
        id: "release-check",
        display_name: "Release Check",
        description: "릴리즈 전 문서, 테스트, 정책 누락을 점검한다.",
        mode: "review-only",
        required_hooks: READ_ONLY_HOOKS,
        allowed_tools: &["read_file", "run_read_only_command"],
        context_requirements: &["release_scope", "test_results"],
        evidence_requirements: &["check_result"],
        stop_criteria: &["release_findings_reported", "korean_report_passed"],
    },
];

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

pub fn find_skill(id: &str) -> Option<&'static SkillManifest> {
    BUILTIN_SKILLS.iter().find(|skill| skill.id == id)
}

pub fn resolve_skill(id: &str) -> Result<Option<ResolvedSkillManifest>, AppError> {
    if let Some(manifest) = find_skill(id) {
        return Ok(Some(ResolvedSkillManifest::Builtin(manifest)));
    }
    crate::plugin::resolve_imported_codex_skill(id)
        .map(|manifest| manifest.map(ResolvedSkillManifest::Imported))
}

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

pub fn validate_transition(current: SkillState, next: SkillState) -> Result<(), AppError> {
    let allowed = matches!(
        (current, next),
        (SkillState::Selected, SkillState::ContextReady)
            | (SkillState::ContextReady, SkillState::ModelRequested)
            | (SkillState::ModelRequested, SkillState::ActionRecorded)
            | (SkillState::ActionRecorded, SkillState::AwaitingApproval)
            | (SkillState::ActionRecorded, SkillState::StopPassed)
            | (
                SkillState::AwaitingApproval,
                SkillState::AwaitingVerification
            )
            | (SkillState::AwaitingVerification, SkillState::StopPassed)
            | (SkillState::StopPassed, SkillState::Complete)
    ) || (!current.is_terminal()
        && matches!(next, SkillState::Failed | SkillState::Cancelled));

    if allowed {
        Ok(())
    } else {
        Err(AppError::blocked(format!(
            "skill state transition 차단\n- current: {}\n- next: {}",
            current.label(),
            next.label()
        )))
    }
}

fn validate_required(
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

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
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
