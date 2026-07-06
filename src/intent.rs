use crate::app::AppError;
use crate::backend;
use crate::context::{self, ContextPack};
use crate::skill;
use crate::state;

const RUN_MAX_TOKENS: u32 = 256;

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
    let intent_event_id = state::record_event(
        "intent.classified",
        "사용자 요청 intent 정규화",
        &format!(
            "skill_id={} mode={} invocation={} signals={:?}",
            decision.skill_id, decision.mode, decision.invocation, decision.signals
        ),
    )?;
    let context_pack = context::build_context_pack(request)?;
    let context_event_id = state::record_event(
        "context.pack.prepared",
        "bounded repository context 준비",
        &format!(
            "files_read={} chars_read={} source_pointers={}",
            context_pack.files_read,
            context_pack.chars_read,
            context_pack.pointer_summary()
        ),
    )?;
    let agent_prompt = agent_loop_prompt(request, &decision, &context_pack);
    let run = backend::chat_once(&agent_prompt, Some(RUN_MAX_TOKENS))?;

    Ok(format!(
        "run agent loop\n- status: model-response-completed\n- request: {}\n- invocation: {}\n- selected skill: {}\n- mode: {}\n- signals: {}\n- constraints: {}\n- classifier: {}\n- workflow ownership: {}\n- context files read: {}\n- context chars: {}\n- source pointers: {}\n- backend: {}\n- model id: {}\n- model path: {}\n- ctx size: {}\n- prompt chars: {}\n- response chars: {}\n- max tokens: {}\n- finish reason: {}\n- guard: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- elapsed ms: {}\n- intent ledger event: {}\n- context ledger event: {}\n- model ledger event: {}\n- boundary: 아직 파일 수정, patch 적용, command 실행은 하지 않습니다. Snippet은 context hint이며 승인된 action 전에는 source pointer 원본을 다시 읽어야 합니다.\n- response:\n{}",
        request,
        decision.invocation,
        decision.skill_id,
        decision.mode,
        display_list(&decision.signals),
        display_list(&decision.constraints),
        decision.classifier,
        state::workflow_ownership_summary(),
        context_pack.files_read,
        context_pack.chars_read,
        context_pack.pointer_summary(),
        run.backend_id,
        run.model_id,
        run.model_path.display(),
        display_optional_u32(run.ctx_size),
        run.prompt_chars,
        run.response_chars,
        run.max_tokens,
        run.finish_reason,
        run.guard_status,
        display_optional_u32(run.prompt_tokens),
        display_optional_u32(run.completion_tokens),
        display_optional_u32(run.total_tokens),
        run.elapsed_ms,
        intent_event_id,
        context_event_id,
        run.ledger_event,
        run.response
    ))
}

pub fn classify_report(request: &str) -> Result<String, AppError> {
    let decision = classify(request)?;
    Ok(format!(
        "intent classify 결과\n- selected skill: {}\n- mode: {}\n- invocation: {}\n- signals: {}\n- constraints: {}\n- classifier: {}\n- workflow ownership: {}\n- repo instruction boundary: AGENTS/HANDOFF 같은 지침은 pointer로만 잡고, 실행 전 원문을 다시 읽어야 합니다.\n- nested/subagent prompt: parent runtime이 전달한 내부 prompt에서는 keyword auto-activation을 하지 않습니다.",
        decision.skill_id,
        decision.mode,
        decision.invocation,
        display_list(&decision.signals),
        display_list(&decision.constraints),
        decision.classifier,
        state::workflow_ownership_summary()
    ))
}

pub fn routes_report() -> String {
    format!(
        "intent route table\n- command palette: request.submit -> rpotato run <request>\n- command palette: intent.preview -> rpotato intent classify <request>\n- command palette: skill.run -> rpotato skill run <id>\n- command palette: plugin.review -> rpotato plugin inspect <id> 또는 rpotato plugin validate <id>\n- command palette: plugin.toggle -> rpotato plugin enable <id> 또는 rpotato plugin disable <id>\n- command palette: workflow.cancel -> rpotato cancel\n- command palette: session.history -> rpotato session list\n- command palette: session.resume -> rpotato resume <session-id>\n- command palette: workflow.resume -> rpotato state resume\n- command palette: monitor.open -> rpotato monitor status\n- command palette: evidence.inspect -> rpotato evidence validate <artifact-pointer>\n- workflow ownership: {}",
        state::workflow_ownership_summary()
    )
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

fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

fn agent_loop_prompt(
    request: &str,
    decision: &IntentDecision,
    context_pack: &ContextPack,
) -> String {
    format!(
        "rpotato run 최소 agent-loop 실행입니다.\n\
         사용자 요청:\n{}\n\n\
         runtime routing:\n\
         - selected skill: {}\n\
         - mode: {}\n\
         - invocation: {}\n\
         - signals: {}\n\
         - constraints: {}\n\n\
         {}\n\
         현재 구현 단계의 경계:\n\
         - 파일 수정, patch 적용, command 실행은 하지 않습니다.\n\
         - context snippet만 근거로 원본 전체를 읽었다고 주장하지 않습니다.\n\
         - 필요한 source pointer, 다음 action candidate, 검증 계획만 한국어로 짧게 제안합니다.\n\
         - 내부 추론이나 <think> 태그를 출력하지 않습니다.",
        request,
        decision.skill_id,
        decision.mode,
        decision.invocation,
        display_list(&decision.signals),
        display_list(&decision.constraints),
        context_pack.prompt_section()
    )
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

    #[test]
    fn routes_report_contains_tui_palette_contract() {
        let report = routes_report();
        assert!(report.contains("command palette"));
        assert!(report.contains("rpotato run"));
    }
}
