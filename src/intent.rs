use crate::app::AppError;
use crate::skill;
use crate::state;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentDecision {
    pub skill_id: String,
    pub mode: &'static str,
    pub invocation: &'static str,
    pub signals: Vec<&'static str>,
    pub constraints: Vec<&'static str>,
    pub classifier: &'static str,
}

pub fn run_report(request: &str) -> Result<String, AppError> {
    let decision = classify(request)?;
    let event_id = state::record_event(
        "intent.classified",
        "사용자 요청 intent 정규화",
        &format!(
            "skill_id={} mode={} invocation={} signals={:?}",
            decision.skill_id, decision.mode, decision.invocation, decision.signals
        ),
    )?;

    Ok(format!(
        "run 계획\n- request: {}\n- invocation: {}\n- selected skill: {}\n- mode: {}\n- signals: {}\n- constraints: {}\n- classifier: {}\n- ledger event: {}\n- 동작: 현재는 intent/skill/mode 정규화까지만 수행하고 model/backend 실행은 후속 phase에서 처리합니다.",
        request,
        decision.invocation,
        decision.skill_id,
        decision.mode,
        display_list(&decision.signals),
        display_list(&decision.constraints),
        decision.classifier,
        event_id
    ))
}

pub fn classify_report(request: &str) -> Result<String, AppError> {
    let decision = classify(request)?;
    Ok(format!(
        "intent classify 결과\n- selected skill: {}\n- mode: {}\n- invocation: {}\n- signals: {}\n- constraints: {}\n- classifier: {}\n- repo instruction boundary: AGENTS/HANDOFF 같은 지침은 pointer로만 잡고, 실행 전 원문을 다시 읽어야 합니다.\n- nested/subagent prompt: parent runtime이 전달한 내부 prompt에서는 keyword auto-activation을 하지 않습니다.",
        decision.skill_id,
        decision.mode,
        decision.invocation,
        display_list(&decision.signals),
        display_list(&decision.constraints),
        decision.classifier
    ))
}

pub fn classify(request: &str) -> Result<IntentDecision, AppError> {
    let trimmed = request.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage("분류할 user request가 필요합니다."));
    }

    if let Some(skill_id) = explicit_skill(trimmed) {
        let Some(manifest) = skill::find_skill(skill_id) else {
            return Err(AppError::usage(format!(
                "explicit skill을 찾지 못했습니다: {skill_id}"
            )));
        };

        return Ok(IntentDecision {
            skill_id: manifest.id.to_string(),
            mode: manifest.mode,
            invocation: "explicit-skill",
            signals: vec!["explicit-invocation"],
            constraints: detect_constraints(trimmed),
            classifier: "deterministic-rules-only",
        });
    }

    let lower = trimmed.to_ascii_lowercase();
    let mut signals = Vec::new();
    let (skill_id, mode) = if has_any(&lower, &["test", "cargo test", "pytest"])
        || has_any(trimmed, &["테스트", "실패"])
    {
        signals.push("test-signal");
        ("fix-test", "execute")
    } else if has_any(&lower, &["review", "code review"]) || has_any(trimmed, &["리뷰", "검토"])
    {
        signals.push("review-only");
        ("code-review", "review-only")
    } else if has_any(&lower, &["plan", "roadmap"]) || has_any(trimmed, &["계획", "로드맵", "설계"])
    {
        signals.push("plan-only");
        ("ontology-refresh", "plan-only")
    } else if has_any(&lower, &["explain", "why", "error"])
        || has_any(trimmed, &["설명", "왜", "에러", "오류"])
    {
        signals.push("explain-error");
        ("explain-error", "read-only")
    } else if has_any(&lower, &["map", "find", "search", "analyze"])
        || has_any(trimmed, &["찾아", "분석", "구조", "어디"])
    {
        signals.push("read-only");
        ("repo-map", "read-only")
    } else {
        signals.push("small-patch-default");
        ("small-patch", "execute")
    };

    if has_any(&lower, &["read-only", "no edit", "do not edit"])
        || has_any(trimmed, &["읽기만", "수정하지마", "건드리지마"])
    {
        signals.push("read-only-constraint");
    }

    if has_any(&lower, &["test spec", "acceptance criteria"])
        || has_any(trimmed, &["테스트 명세", "인수 기준"])
    {
        signals.push("test-spec");
    }

    if has_any(
        &lower,
        &["generate", "create file", "write doc", "make document"],
    ) || has_any(trimmed, &["문서 만들어", "파일 만들어", "생성해", "작성해"])
    {
        signals.push("generated-artifact");
    }

    Ok(IntentDecision {
        skill_id: skill_id.to_string(),
        mode,
        invocation: "deterministic-phrase",
        signals,
        constraints: detect_constraints(trimmed),
        classifier: "deterministic-rules-only; optional model classifier disabled",
    })
}

fn explicit_skill(request: &str) -> Option<&str> {
    let rest = request.strip_prefix('$')?;
    let skill_id = rest.split_whitespace().next()?;
    if skill_id.is_empty() {
        None
    } else {
        Some(skill_id)
    }
}

fn detect_constraints(request: &str) -> Vec<&'static str> {
    let lower = request.to_ascii_lowercase();
    let mut constraints = Vec::new();

    if has_any(&lower, &["no external contributor", "no pr"])
        || has_any(request, &["외부기여자", "외부 PR"])
    {
        constraints.push("no-external-contribution");
    }

    if has_any(&lower, &["korean", "hangul"]) || has_any(request, &["한국어", "한글"]) {
        constraints.push("korean-output");
    }

    if has_any(&lower, &["do not browse", "offline"]) || has_any(request, &["검색하지마"]) {
        constraints.push("no-network-retrieval");
    }

    constraints
}

fn has_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn display_list(values: &[&str]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_skill_has_priority() {
        let decision = classify("$fix-test 리뷰만 해줘").unwrap();
        assert_eq!(decision.invocation, "explicit-skill");
        assert_eq!(decision.skill_id, "fix-test");
    }

    #[test]
    fn detects_review_only_signal() {
        let decision = classify("이 변경 리뷰해줘").unwrap();
        assert_eq!(decision.skill_id, "code-review");
        assert_eq!(decision.mode, "review-only");
    }

    #[test]
    fn detects_test_spec_signal() {
        let decision = classify("테스트 명세를 만들어줘").unwrap();
        assert!(decision.signals.contains(&"test-spec"));
    }

    #[test]
    fn detects_generated_artifact_signal() {
        let decision = classify("문서 만들어줘").unwrap();
        assert!(decision.signals.contains(&"generated-artifact"));
    }
}
