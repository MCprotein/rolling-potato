use crate::app::AppError;
use crate::state;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillManifest {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub mode: &'static str,
    pub allowed_tools: &'static [&'static str],
    pub context_requirements: &'static [&'static str],
    pub evidence_requirements: &'static [&'static str],
    pub stop_criteria: &'static [&'static str],
}

pub const BUILTIN_SKILLS: &[SkillManifest] = &[
    SkillManifest {
        id: "fix-test",
        display_name: "Fix Test",
        description: "실패한 테스트 하나를 좁은 범위에서 수정하고 검증한다.",
        mode: "execute",
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
        allowed_tools: &["read_file", "render_diff", "apply_patch"],
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
        allowed_tools: &["read_file", "run_read_only_command"],
        context_requirements: &["release_scope", "test_results"],
        evidence_requirements: &["check_result"],
        stop_criteria: &["release_findings_reported", "korean_report_passed"],
    },
];

pub fn list_report() -> String {
    let skills = BUILTIN_SKILLS
        .iter()
        .map(|skill| {
            format!(
                "- {} ({}) | mode: {} | {}",
                skill.id, skill.display_name, skill.mode, skill.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "skill registry\n- native skills: {}\n- imported skill namespace: imported.<plugin>.<skill>\n- 실행 경계: skill은 tool을 직접 실행하지 않고 runtime policy/evidence gate를 통과해야 합니다.\n{}",
        BUILTIN_SKILLS.len(),
        skills
    )
}

pub fn run_report(id: &str) -> Result<String, AppError> {
    let Some(skill) = find_skill(id) else {
        return Err(AppError::usage(format!(
            "등록된 skill을 찾지 못했습니다: {id}\n확인: rpotato skill list"
        )));
    };

    let details = format!(
        "skill_id={} mode={} context={:?} evidence={:?}",
        skill.id, skill.mode, skill.context_requirements, skill.evidence_requirements
    );
    let event_id =
        state::record_event("skill.run.normalized", "skill invocation 정규화", &details)?;

    Ok(format!(
        "skill run 계획\n- skill id: {}\n- display: {}\n- mode: {}\n- allowed tools: {}\n- context requirements: {}\n- evidence requirements: {}\n- stop criteria: {}\n- ledger event: {}\n- 동작: 현재는 invocation normalization만 수행하고 agent loop 실행은 후속 phase에서 처리합니다.",
        skill.id,
        skill.display_name,
        skill.mode,
        skill.allowed_tools.join(", "),
        skill.context_requirements.join(", "),
        skill.evidence_requirements.join(", "),
        skill.stop_criteria.join(", "),
        event_id
    ))
}

pub fn find_skill(id: &str) -> Option<&'static SkillManifest> {
    BUILTIN_SKILLS.iter().find(|skill| skill.id == id)
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
        assert!(report.contains("imported.<plugin>.<skill>"));
    }
}
