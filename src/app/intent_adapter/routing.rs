use crate::app::extensions_adapter::skill;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::patch::intent::{
    self as intent_domain, detect_constraints, display_list, IntentDecision, IntentSkill,
};

use super::execution;

pub fn run_report(request: &str) -> Result<String, AppError> {
    let decision = classify(request)?;
    let manifest = skill::resolve_skill(&decision.skill_id)?
        .ok_or_else(|| AppError::blocked("selected skill manifest가 사라졌습니다."))?;
    execution::run_with_decision(request, decision, manifest)
}

pub fn run_skill_report(skill_id: &str, request: &str) -> Result<String, AppError> {
    let request = request.trim();
    if request.is_empty() {
        return Err(AppError::usage("skill run request가 필요합니다."));
    }
    let Some(manifest) = skill::resolve_skill(skill_id)? else {
        return Err(AppError::usage(format!(
            "등록된 skill을 찾지 못했습니다: {skill_id}\n확인: rpotato skill list"
        )));
    };
    let decision = IntentDecision {
        skill_id: manifest.id().to_string(),
        mode: manifest.mode(),
        invocation: "explicit-skill",
        signals: vec!["explicit-invocation"],
        constraints: detect_constraints(request),
        classifier: if manifest.imported().is_some() {
            "explicit-imported-skill"
        } else {
            "explicit-built-in-skill"
        },
    };
    execution::run_with_decision(request, decision, manifest)
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
    intent_domain::classify(request, |skill_id| {
        skill::find_skill(skill_id).map(|manifest| IntentSkill {
            id: manifest.id.to_string(),
            mode: manifest.mode,
        })
    })
}
