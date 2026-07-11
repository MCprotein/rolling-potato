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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActionCandidate {
    kind: &'static str,
    approval_required: bool,
    next_gate: &'static str,
    allowed_side_effects: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedModelAction {
    status: &'static str,
    kind: String,
    source_pointers: String,
    next_gate: String,
    requested_side_effects: String,
    executable_now: bool,
    target_path: String,
    find_text: String,
    replace_text: String,
    verification_command: String,
}

pub fn run_report(request: &str) -> Result<String, AppError> {
    if let Some(workflow_id) = state::active_workflow_id()? {
        return crate::patch::resume_workflow_report(&workflow_id);
    }
    let decision = classify(request)?;
    let mut workflow = state::create_workflow(request)?;
    let intent_event_id = state::record_event(
        "intent.classified",
        "사용자 요청 intent 정규화",
        &format!(
            "skill_id={} mode={} invocation={} signals={:?}",
            decision.skill_id, decision.mode, decision.invocation, decision.signals
        ),
    )?;
    let context_pack = context::build_context_pack(request)?;
    let action_candidate = plan_action_candidate(&decision, &context_pack);
    let context_event_id = state::record_event(
        "context.pack.prepared",
        "bounded repository context 준비",
        &format!(
            "origin={} ontology_selected={} stale_rejected={} files_read={} chars_read={} source_pointers={}",
            context_pack.origin,
            context_pack.ontology_records_selected,
            context_pack.ontology_stale_rejected,
            context_pack.files_read,
            context_pack.chars_read,
            context_pack.pointer_summary()
        ),
    )?;
    let action_event_id = state::record_event(
        "action.candidate.prepared",
        "run action candidate 준비",
        &format!(
            "kind={} approval_required={} next_gate={} source_pointers={}",
            action_candidate.kind,
            action_candidate.approval_required,
            action_candidate.next_gate,
            context_pack.pointer_summary()
        ),
    )?;
    let agent_prompt = agent_loop_prompt(request, &decision, &context_pack, &action_candidate);
    let run = match backend::chat_once(&agent_prompt, Some(RUN_MAX_TOKENS)) {
        Ok(run) => run,
        Err(err) => {
            workflow.phase = "failed".to_string();
            workflow.failure_reason = "backend-call-failed".to_string();
            return Err(checkpoint_failure_or_original(workflow, err));
        }
    };
    let model_action = parse_model_action(&run.response, &action_candidate, &context_pack);
    let model_action_event_id = state::record_event(
        "model.action.parsed",
        "model response action parsing",
        &format!(
            "status={} kind={} source_pointers={} next_gate={} requested_side_effects={} executable_now={}",
            model_action.status,
            model_action.kind,
            model_action.source_pointers,
            model_action.next_gate,
            model_action.requested_side_effects,
            model_action.executable_now
        ),
    )?;

    workflow.action_kind = model_action.kind.clone();
    workflow.action_status = model_action.status.to_string();
    workflow.source_path = model_action.target_path.clone();
    workflow.find_text = model_action.find_text.clone();
    workflow.replace_text = model_action.replace_text.clone();
    workflow.verification_plan = model_action.verification_command.clone();
    workflow.phase = "action-recorded".to_string();
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;

    if is_non_mutating_action(&model_action.kind) {
        let pointers_are_valid = model_action.kind != "inspect-sources"
            || (!matches!(model_action.source_pointers.as_str(), "none" | "unverified"));
        let action_is_safe = model_action.status == "parsed"
            && model_action.requested_side_effects == "none"
            && pointers_are_valid;
        if !action_is_safe {
            workflow.phase = "failed".to_string();
            workflow.failure_reason = "invalid-or-hostile-model-action".to_string();
            workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
            state::clear_terminal_workflow_pointer(&workflow)?;
            return Err(AppError::blocked(format!(
                "run agent loop 차단\n- workflow id: {}\n- 이유: 읽기 전용 model action이 runtime 계약을 충족하지 못했습니다.\n- model side effect 실행: 없음",
                workflow.workflow_id
            )));
        }

        workflow.phase = "complete".to_string();
        workflow.action_status = "complete".to_string();
        workflow.approval_state = "not-required".to_string();
        workflow.result_summary = "non-mutating action completed".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
        state::clear_terminal_workflow_pointer(&workflow)?;
        return Ok(render_non_mutating_report(
            request,
            &decision,
            &context_pack,
            &model_action,
            &run.response,
            &workflow,
        ));
    }

    let expected_pointer = format!("{}:1", model_action.target_path);
    let action_is_safe = model_action.status == "parsed"
        && model_action.kind == "patch-proposal"
        && model_action.requested_side_effects == "none"
        && !model_action.target_path.is_empty()
        && !model_action.find_text.is_empty()
        && !model_action.verification_command.is_empty()
        && model_action
            .source_pointers
            .split(',')
            .map(str::trim)
            .any(|pointer| pointer == expected_pointer);
    if !action_is_safe {
        workflow.phase = "failed".to_string();
        workflow.failure_reason = "invalid-or-hostile-model-action".to_string();
        workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;
        return Err(AppError::blocked(format!(
            "run agent loop 차단\n- workflow id: {}\n- 이유: model action은 non-executable record로 저장했지만 안전한 patch proposal 계약을 충족하지 못했습니다.\n- model side effect 실행: 없음",
            workflow.workflow_id
        )));
    }

    let proposal = match crate::patch::prepare_workflow_proposal(
        &workflow.workflow_id,
        &workflow.action_id,
        &model_action.target_path,
        &model_action.find_text,
        &model_action.replace_text,
        &model_action.verification_command,
    ) {
        Ok(proposal) => proposal,
        Err(err) => {
            workflow.phase = "failed".to_string();
            workflow.failure_reason = "proposal-preparation-failed".to_string();
            return Err(checkpoint_failure_or_original(workflow, err));
        }
    };
    workflow.source_path = proposal.relative_path.clone();
    workflow.source_hash = proposal.original_sha256.clone();
    workflow.proposal_id = proposal.proposal_id.clone();
    workflow.proposal_hash = proposal.proposal_hash.clone();
    workflow.approval_credential_hash = proposal.approval_credential_hash.clone();
    workflow.before_hash = proposal.original_sha256.clone();
    workflow.after_hash = proposal.proposed_sha256.clone();
    workflow.verification_plan = proposal.verification_command.clone();
    workflow.approval_state = "pending".to_string();
    workflow.result_summary = "patch proposal awaiting apply approval".to_string();
    workflow.phase = "pending-approval".to_string();
    workflow = state::checkpoint_workflow(workflow.clone(), workflow.revision)?;

    Ok(format!(
        "run agent loop\n- status: pending-approval\n- request: {}\n- invocation: {}\n- selected skill: {}\n- mode: {}\n- signals: {}\n- constraints: {}\n- classifier: {}\n- workflow ownership: {}\n- context origin: {}\n- ontology records selected: {}\n- ontology stale rejected: {}\n- context files read: {}\n- context chars: {}\n- source pointers: {}\n- action candidate: {}\n- approval required before side effect: {}\n- next gate: {}\n- allowed side effects now: {}\n- model action parse: {}\n- model action kind: {}\n- model action source pointers: {}\n- model action next gate: {}\n- model action requested side effects: {}\n- model action executable now: {}\n- backend: {}\n- model id: {}\n- model path: {}\n- ctx size: {}\n- prompt chars: {}\n- response chars: {}\n- requested max tokens: {}\n- effective max tokens: {}\n- resource governor admission: {}\n- resource governor token action: {}\n- resource governor reason: {}\n- finish reason: {}\n- guard: {}\n- prompt tokens: {}\n- completion tokens: {}\n- total tokens: {}\n- elapsed ms: {}\n- intent ledger event: {}\n- context ledger event: {}\n- action ledger event: {}\n- model action ledger event: {}\n- model ledger event: {}\n- workflow id: {}\n- workflow revision: {}\n- proposal id: {}\n- verification plan: {}\n- approval command: rpotato patch approve {} --token {}\n- boundary: model output은 실행되지 않았고 ontology source pointer에서 원본 source를 다시 읽어 diff만 만들었습니다.\n- response:\n{}\n- diff:\n{}",
        request,
        decision.invocation,
        decision.skill_id,
        decision.mode,
        display_list(&decision.signals),
        display_list(&decision.constraints),
        decision.classifier,
        state::workflow_ownership_summary(),
        context_pack.origin,
        context_pack.ontology_records_selected,
        context_pack.ontology_stale_rejected,
        context_pack.files_read,
        context_pack.chars_read,
        context_pack.pointer_summary(),
        action_candidate.kind,
        display_bool(action_candidate.approval_required),
        action_candidate.next_gate,
        action_candidate.allowed_side_effects,
        model_action.status,
        model_action.kind,
        model_action.source_pointers,
        model_action.next_gate,
        model_action.requested_side_effects,
        display_bool(model_action.executable_now),
        run.backend_id,
        run.model_id,
        run.model_path.display(),
        display_optional_u32(run.ctx_size),
        run.prompt_chars,
        run.response_chars,
        run.requested_max_tokens,
        run.effective_max_tokens,
        run.resource_governor_admission,
        run.resource_governor_token_action,
        run.resource_governor_reason,
        run.finish_reason,
        run.guard_status,
        display_optional_u32(run.prompt_tokens),
        display_optional_u32(run.completion_tokens),
        display_optional_u32(run.total_tokens),
        run.elapsed_ms,
        intent_event_id,
        context_event_id,
        action_event_id,
        model_action_event_id,
        run.ledger_event,
        workflow.workflow_id,
        workflow.revision,
        proposal.proposal_id,
        crate::ledger::redact_text(&proposal.verification_command),
        proposal.proposal_id,
        proposal.approval_token,
        run.response,
        proposal.diff
    ))
}

fn is_non_mutating_action(kind: &str) -> bool {
    matches!(
        kind,
        "answer-only" | "inspect-sources" | "generated-artifact-plan"
    )
}

fn render_non_mutating_report(
    request: &str,
    decision: &IntentDecision,
    context_pack: &ContextPack,
    model_action: &ParsedModelAction,
    response: &str,
    workflow: &state::WorkflowRecord,
) -> String {
    let answer = model_answer(response);
    format!(
        "run 결과\n- 상태: 완료\n- 요청: {}\n- 선택한 skill: {}\n- mode: {}\n- workflow id: {}\n- workflow kind: {}\n- action id: {}\n- action kind: {}\n- context origin: {}\n- ontology records selected: {}\n- ontology stale rejected: {}\n- source pointers: {}\n- context files read: {}\n- side effect: 없음\n- approval: 불필요\n- 답변:\n{}",
        request,
        decision.skill_id,
        decision.mode,
        workflow.workflow_id,
        workflow.workflow_kind,
        workflow.action_id,
        workflow.action_kind,
        context_pack.origin,
        context_pack.ontology_records_selected,
        context_pack.ontology_stale_rejected,
        model_action.source_pointers,
        context_pack.files_read,
        answer
    )
}

fn model_answer(response: &str) -> String {
    let without_thinking = strip_thinking_sections(response);
    let visible = without_thinking
        .lines()
        .filter(|line| model_action_body(line).is_none())
        .collect::<Vec<_>>()
        .join("\n");
    let visible = visible.trim();
    if visible.is_empty() {
        "요청을 읽기 전용으로 처리했으며 실행할 변경은 없습니다.".to_string()
    } else {
        crate::korean_guard::guard_or_failure(visible)
    }
}

fn strip_thinking_sections(response: &str) -> String {
    let mut remaining = response;
    let mut visible = String::new();
    loop {
        let Some(start) = remaining.find("<think>") else {
            visible.push_str(remaining);
            break;
        };
        visible.push_str(&remaining[..start]);
        let after_start = &remaining[start + "<think>".len()..];
        let Some(end) = after_start.find("</think>") else {
            break;
        };
        remaining = &after_start[end + "</think>".len()..];
    }
    visible
}

fn checkpoint_failure_or_original(workflow: state::WorkflowRecord, original: AppError) -> AppError {
    match state::checkpoint_workflow(workflow.clone(), workflow.revision) {
        Ok(_) => original,
        Err(persistence) => {
            let _ = state::record_validation_gap(
                "workflow-failure-checkpoint",
                &format!("{}:{}", workflow.workflow_id, workflow.failure_reason),
            );
            AppError {
                code: original.code,
                message: format!(
                    "{}\n- failure checkpoint: 저장 실패\n- persistence error: {}",
                    original.message, persistence.message
                ),
            }
        }
    }
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

fn display_bool(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn plan_action_candidate(decision: &IntentDecision, context_pack: &ContextPack) -> ActionCandidate {
    let has_context = !context_pack.source_pointers.is_empty();
    if matches!(decision.mode, "read-only" | "review-only" | "plan-only") {
        return ActionCandidate {
            kind: if has_context {
                "inspect-sources"
            } else {
                "answer-only"
            },
            approval_required: false,
            next_gate: "source-reread-before-claim",
            allowed_side_effects: "none",
        };
    }

    if decision.signals.contains(&"generated-artifact") {
        return ActionCandidate {
            kind: "generated-artifact-plan",
            approval_required: true,
            next_gate: "diff-before-write",
            allowed_side_effects: "none",
        };
    }

    if matches!(decision.skill_id.as_str(), "fix-test" | "small-patch") {
        return ActionCandidate {
            kind: "patch-proposal",
            approval_required: true,
            next_gate: "diff-before-write",
            allowed_side_effects: "none",
        };
    }

    ActionCandidate {
        kind: "answer-only",
        approval_required: false,
        next_gate: "korean-output-guard",
        allowed_side_effects: "none",
    }
}

fn parse_model_action(
    response: &str,
    runtime_candidate: &ActionCandidate,
    context_pack: &ContextPack,
) -> ParsedModelAction {
    let Some(fields) = parse_model_action_fields(response) else {
        return parse_model_action_text(response, runtime_candidate, context_pack).unwrap_or_else(
            || fallback_model_action("missing-model-action-line", runtime_candidate),
        );
    };
    let raw_kind = field_value(&fields, &["kind"]).unwrap_or_default();
    let Some(parsed_kind) = normalize_model_action_kind(&raw_kind) else {
        return fallback_model_action("unknown-model-action-kind", runtime_candidate);
    };
    let raw_side_effects = field_value(&fields, &["side_effects", "allowed_side_effects"])
        .unwrap_or_else(|| runtime_candidate.allowed_side_effects.to_string());
    let side_effects = normalize_side_effects(&raw_side_effects);
    if side_effects != "none" {
        let mut blocked = fallback_model_action("blocked-side-effect-request", runtime_candidate);
        blocked.requested_side_effects = side_effects;
        return blocked;
    }
    if parsed_kind != runtime_candidate.kind {
        return fallback_model_action("mismatch-runtime-fallback", runtime_candidate);
    }

    let raw_source_pointers =
        field_value(&fields, &["source_pointers", "sources"]).unwrap_or_else(|| "none".to_string());
    let raw_next_gate = field_value(&fields, &["next_gate"])
        .unwrap_or_else(|| runtime_candidate.next_gate.to_string());

    ParsedModelAction {
        status: "parsed",
        kind: parsed_kind.to_string(),
        source_pointers: normalize_source_pointers(&raw_source_pointers, context_pack),
        next_gate: normalize_next_gate(&raw_next_gate, runtime_candidate),
        requested_side_effects: side_effects,
        executable_now: false,
        target_path: field_value(&fields, &["path", "target_path"]).unwrap_or_default(),
        find_text: decode_action_text(field_value(&fields, &["find_hex"]).as_deref()),
        replace_text: decode_action_text(field_value(&fields, &["replace_hex"]).as_deref()),
        verification_command: field_value(&fields, &["verification", "verification_command"])
            .unwrap_or_default(),
    }
}

fn parse_model_action_text(
    response: &str,
    runtime_candidate: &ActionCandidate,
    context_pack: &ContextPack,
) -> Option<ParsedModelAction> {
    let parsed_kind = normalize_model_action_kind(response)?;
    if parsed_kind != runtime_candidate.kind {
        return Some(fallback_model_action(
            "heuristic-runtime-fallback",
            runtime_candidate,
        ));
    }

    Some(ParsedModelAction {
        status: "heuristic-text",
        kind: parsed_kind.to_string(),
        source_pointers: source_pointers_from_text(response, context_pack),
        next_gate: next_gate_from_text(response, runtime_candidate),
        requested_side_effects: runtime_candidate.allowed_side_effects.to_string(),
        executable_now: false,
        target_path: String::new(),
        find_text: String::new(),
        replace_text: String::new(),
        verification_command: String::new(),
    })
}

fn parse_model_action_fields(response: &str) -> Option<Vec<(String, String)>> {
    let line = response.lines().rev().find_map(model_action_body)?;
    let fields = line
        .split(';')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            let key = key.trim().to_ascii_lowercase().replace('-', "_");
            let value = value.trim().to_string();
            if key.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect::<Vec<_>>();

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

fn model_action_body(line: &str) -> Option<&str> {
    let trimmed = line
        .trim()
        .strip_prefix("- ")
        .unwrap_or_else(|| line.trim())
        .trim()
        .trim_matches('`');
    if let Some((prefix, body)) = trimmed.split_once(':') {
        let normalized_prefix = prefix.trim().to_ascii_lowercase();
        if normalized_prefix == "model action" || prefix.trim() == "모델액션" {
            return Some(body.trim());
        }
    }
    None
}

fn field_value(fields: &[(String, String)], names: &[&str]) -> Option<String> {
    fields
        .iter()
        .find(|(key, _)| names.iter().any(|name| key == name))
        .map(|(_, value)| value.clone())
}

fn normalize_model_action_kind(value: &str) -> Option<&'static str> {
    let lower = value.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    if lower == "patch-proposal" || lower.contains("patch") || value.contains("패치") {
        Some("patch-proposal")
    } else if lower == "inspect-sources"
        || lower.contains("inspect")
        || lower.contains("source")
        || value.contains("소스")
        || value.contains("원본")
    {
        Some("inspect-sources")
    } else if lower == "generated-artifact-plan"
        || lower.contains("artifact")
        || lower.contains("generate")
        || value.contains("문서")
        || value.contains("생성")
    {
        Some("generated-artifact-plan")
    } else if lower == "answer-only" || lower.contains("answer") || value.contains("답변") {
        Some("answer-only")
    } else {
        None
    }
}

fn normalize_source_pointers(value: &str, context_pack: &ContextPack) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("none")
        || trimmed == "없음"
        || trimmed == "-"
    {
        return "none".to_string();
    }

    let verified = trimmed
        .split(',')
        .map(str::trim)
        .filter(|pointer| {
            context_pack
                .source_pointers
                .iter()
                .any(|source| source.stable_ref == *pointer)
        })
        .take(4)
        .map(str::to_string)
        .collect::<Vec<_>>();

    if verified.is_empty() {
        "unverified".to_string()
    } else {
        verified.join(", ")
    }
}

fn source_pointers_from_text(response: &str, context_pack: &ContextPack) -> String {
    let pointers = context_pack
        .source_pointers
        .iter()
        .filter(|source| response.contains(&source.stable_ref))
        .take(4)
        .map(|source| source.stable_ref.clone())
        .collect::<Vec<_>>();

    if pointers.is_empty() {
        "none".to_string()
    } else {
        pointers.join(", ")
    }
}

fn next_gate_from_text(_response: &str, runtime_candidate: &ActionCandidate) -> String {
    runtime_candidate.next_gate.to_string()
}

fn normalize_next_gate(value: &str, runtime_candidate: &ActionCandidate) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        runtime_candidate.next_gate.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_side_effects(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('.');
    if trimmed.eq_ignore_ascii_case("none") || trimmed == "없음" || trimmed == "-" {
        "none".to_string()
    } else {
        trimmed.to_string()
    }
}

fn fallback_model_action(
    status: &'static str,
    runtime_candidate: &ActionCandidate,
) -> ParsedModelAction {
    ParsedModelAction {
        status,
        kind: runtime_candidate.kind.to_string(),
        source_pointers: "none".to_string(),
        next_gate: runtime_candidate.next_gate.to_string(),
        requested_side_effects: runtime_candidate.allowed_side_effects.to_string(),
        executable_now: false,
        target_path: String::new(),
        find_text: String::new(),
        replace_text: String::new(),
        verification_command: String::new(),
    }
}

fn decode_action_text(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if !value.len().is_multiple_of(2) {
        return String::new();
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let Ok(hex) = std::str::from_utf8(pair) else {
            return String::new();
        };
        let Ok(byte) = u8::from_str_radix(hex, 16) else {
            return String::new();
        };
        bytes.push(byte);
    }
    String::from_utf8(bytes).unwrap_or_default()
}

fn agent_loop_prompt(
    request: &str,
    decision: &IntentDecision,
    context_pack: &ContextPack,
    action_candidate: &ActionCandidate,
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
         runtime action candidate:\n\
         - kind: {}\n\
         - approval required before side effect: {}\n\
         - next gate: {}\n\
         - allowed side effects now: {}\n\n\
         model response action contract:\n\
         - 마지막 줄은 반드시 아래 형식으로 씁니다.\n\
         - find/replace는 UTF-8 bytes의 lowercase hex로 인코딩합니다.\n\
         - verification은 shell operator 없는 policy-allowed 단순 argv 명령입니다.\n\
         - MODEL ACTION: kind={}; source_pointers={}; path=<project-relative-path>; find_hex=<hex>; replace_hex=<hex>; verification=<command>; next_gate={}; side_effects=none\n\n\
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
        action_candidate.kind,
        display_bool(action_candidate.approval_required),
        action_candidate.next_gate,
        action_candidate.allowed_side_effects,
        action_candidate.kind,
        context_pack.pointer_summary(),
        action_candidate.next_gate,
        context_pack.prompt_section()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::SourcePointer;
    use std::path::PathBuf;

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

    #[test]
    fn execute_mode_plans_patch_proposal_without_side_effects() {
        let decision = classify("테스트 실패 고쳐줘").unwrap();
        let pack = sample_context_pack();

        let candidate = plan_action_candidate(&decision, &pack);

        assert_eq!(candidate.kind, "patch-proposal");
        assert!(candidate.approval_required);
        assert_eq!(candidate.next_gate, "diff-before-write");
        assert_eq!(candidate.allowed_side_effects, "none");
    }

    #[test]
    fn read_only_mode_plans_source_inspection_without_approval() {
        let decision = classify("구조 분석해줘").unwrap();
        let pack = sample_context_pack();

        let candidate = plan_action_candidate(&decision, &pack);

        assert_eq!(candidate.kind, "inspect-sources");
        assert!(!candidate.approval_required);
        assert_eq!(candidate.next_gate, "source-reread-before-claim");
    }

    #[test]
    fn parses_structured_model_action_without_execution() {
        let decision = classify("테스트 실패 고쳐줘").unwrap();
        let pack = sample_context_pack();
        let candidate = plan_action_candidate(&decision, &pack);

        let parsed = parse_model_action(
            "수정 후보만 제안합니다.\nMODEL ACTION: kind=patch-proposal; source_pointers=src/main.rs:1; next_gate=diff-before-write; side_effects=none",
            &candidate,
            &pack,
        );

        assert_eq!(parsed.status, "parsed");
        assert_eq!(parsed.kind, "patch-proposal");
        assert_eq!(parsed.source_pointers, "src/main.rs:1");
        assert_eq!(parsed.next_gate, "diff-before-write");
        assert_eq!(parsed.requested_side_effects, "none");
        assert!(!parsed.executable_now);
    }

    #[test]
    fn model_action_parser_falls_back_on_runtime_mismatch() {
        let decision = classify("테스트 실패 고쳐줘").unwrap();
        let pack = sample_context_pack();
        let candidate = plan_action_candidate(&decision, &pack);

        let parsed = parse_model_action(
            "MODEL ACTION: kind=answer-only; source_pointers=none; next_gate=korean-output-guard; side_effects=none",
            &candidate,
            &pack,
        );

        assert_eq!(parsed.status, "mismatch-runtime-fallback");
        assert_eq!(parsed.kind, "patch-proposal");
        assert_eq!(parsed.next_gate, "diff-before-write");
        assert!(!parsed.executable_now);
    }

    #[test]
    fn model_action_parser_blocks_requested_side_effects() {
        let decision = classify("테스트 실패 고쳐줘").unwrap();
        let pack = sample_context_pack();
        let candidate = plan_action_candidate(&decision, &pack);

        let parsed = parse_model_action(
            "MODEL ACTION: kind=patch-proposal; source_pointers=src/main.rs:1; next_gate=diff-before-write; side_effects=write-file",
            &candidate,
            &pack,
        );

        assert_eq!(parsed.status, "blocked-side-effect-request");
        assert_eq!(parsed.kind, "patch-proposal");
        assert_eq!(parsed.requested_side_effects, "write-file");
        assert!(!parsed.executable_now);
    }

    #[test]
    fn model_action_parser_uses_heuristic_text_when_action_line_is_missing() {
        let decision = classify("테스트 실패 고쳐줘").unwrap();
        let pack = sample_context_pack();
        let candidate = plan_action_candidate(&decision, &pack);

        let parsed = parse_model_action(
            "현재 단계에서 제안되는 action candidate는 'patch-proposal'이며 diff-before-write 게이트 전에는 실행하지 않습니다.",
            &candidate,
            &pack,
        );

        assert_eq!(parsed.status, "heuristic-text");
        assert_eq!(parsed.kind, "patch-proposal");
        assert_eq!(parsed.next_gate, "diff-before-write");
        assert!(!parsed.executable_now);
    }

    #[test]
    fn model_answer_hides_action_contract_and_thinking() {
        let answer = model_answer(
            "<think>internal plan</think>\n구조를 확인했으며 변경은 필요하지 않습니다.\nMODEL ACTION: kind=answer-only; source_pointers=none; next_gate=korean-output-guard; side_effects=none",
        );

        assert_eq!(answer, "구조를 확인했으며 변경은 필요하지 않습니다.");
        assert!(!answer.contains("MODEL ACTION"));
        assert!(!answer.contains("internal plan"));
    }

    fn sample_context_pack() -> ContextPack {
        ContextPack {
            project_root: PathBuf::from("/tmp/project"),
            origin: "ontology".to_string(),
            ontology_records_selected: 1,
            ontology_stale_rejected: 0,
            files_considered: 1,
            files_read: 1,
            chars_read: 12,
            dropped_files: 0,
            source_pointers: vec![SourcePointer {
                path: "src/main.rs".to_string(),
                stable_ref: "src/main.rs:1".to_string(),
                chars: 12,
                fingerprint: "abc".to_string(),
                snippet: "fn main() {}".to_string(),
            }],
        }
    }
}
