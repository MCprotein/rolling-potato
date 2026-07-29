use crate::app::context_adapter::ContextPack;
use crate::app::extensions_adapter::skill;
use crate::runtime_core::patch::intent::ParsedModelAction;

pub(super) fn record_non_mutating_outcomes(
    manifest: &skill::ResolvedSkillManifest,
    context_pack: &ContextPack,
    model_action: &ParsedModelAction,
    answer: &str,
    runtime: &mut skill::SkillRuntimeState,
) {
    let has_pointer = !context_pack.source_pointers.is_empty()
        && !matches!(model_action.source_pointers.as_str(), "none" | "unverified");
    let has_file_reference = has_pointer
        && context_pack
            .source_pointers
            .iter()
            .any(|pointer| answer.contains(&pointer.path));
    let has_file_line_reference = context_pack
        .source_pointers
        .iter()
        .any(|pointer| contains_file_line_reference(answer, &pointer.path));
    let lower = answer.to_ascii_lowercase();
    let has_ranked_findings = ["[high]", "[medium]", "[low]", "[critical]"]
        .iter()
        .any(|marker| lower.contains(marker))
        || ["[심각]", "[높음]", "[중간]", "[낮음]"]
            .iter()
            .any(|marker| answer.contains(marker));
    let has_no_findings = answer.contains("발견 사항 없음") || answer.contains("문제 없음");
    for requirement in manifest.evidence_requirements() {
        let satisfied = match *requirement {
            "source_reference" | "file_reference" => has_file_reference,
            "file_line_reference" => has_file_line_reference,
            "benchmark_source" => {
                has_file_reference && (lower.contains("benchmark") || answer.contains("벤치마크"))
            }
            "source_url_or_file" => has_file_reference || lower.contains("https://"),
            "confidence_record" => {
                has_file_reference && (lower.contains("confidence") || answer.contains("신뢰도"))
            }
            "diagnostic_output" => {
                lower.contains("diagnostic") || answer.contains("진단") || answer.contains("상태")
            }
            "check_result" => {
                lower.contains("pass")
                    || lower.contains("fail")
                    || answer.contains("통과")
                    || answer.contains("실패")
                    || answer.contains("점검")
            }
            "checksum_record" => lower.contains("sha256"),
            "local_result_artifact" => false,
            _ => false,
        };
        if satisfied {
            runtime.record_evidence(requirement);
        }
    }

    for criterion in manifest.stop_criteria() {
        let satisfied = match *criterion {
            "korean_report_passed" => {
                crate::runtime_core::reporting::korean_guard::validate(answer)
            }
            "claims_source_backed" => manifest
                .evidence_requirements()
                .iter()
                .all(|required| runtime.evidence.iter().any(|actual| actual == required)),
            "cause_explained" => {
                runtime
                    .evidence
                    .iter()
                    .any(|value| value == "source_reference")
                    && (answer.contains("원인")
                        || answer.contains("이유")
                        || answer.contains("때문"))
            }
            "findings_ranked" => {
                runtime
                    .evidence
                    .iter()
                    .any(|value| value == "file_line_reference")
                    && (has_ranked_findings || has_no_findings)
            }
            "map_reported" => runtime
                .evidence
                .iter()
                .any(|value| value == "file_reference"),
            "benchmark_plan_ready" => {
                runtime
                    .evidence
                    .iter()
                    .any(|value| value == "benchmark_source")
                    && (lower.contains("plan") || answer.contains("계획"))
            }
            "diagnosis_reported" => runtime
                .evidence
                .iter()
                .any(|value| value == "diagnostic_output"),
            "ontology_delta_ready" => {
                runtime
                    .evidence
                    .iter()
                    .any(|value| value == "source_reference")
                    && (lower.contains("delta")
                        || answer.contains("변경")
                        || answer.contains("갱신"))
            }
            "release_findings_reported" => {
                runtime.evidence.iter().any(|value| value == "check_result")
            }
            _ => false,
        };
        if satisfied {
            runtime.record_stop_criterion(criterion);
        }
    }
}

fn contains_file_line_reference(answer: &str, path: &str) -> bool {
    let mut remaining = answer;
    while let Some(index) = remaining.find(path) {
        let suffix = &remaining[index + path.len()..];
        if suffix
            .strip_prefix(':')
            .is_some_and(|value| value.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        {
            return true;
        }
        remaining = &suffix[suffix.chars().next().map(char::len_utf8).unwrap_or(0)..];
    }
    false
}
