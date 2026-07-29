use crate::foundation::error::AppError;
use crate::runtime_core::terminal::TerminalIo;

use super::super::super::outcome::{exact_tui_outcome, TuiOutcomeCode, TuiOutcomeContext};
use super::super::super::runtime_bridge::{OneShotSecret, TuiIntent};
use super::super::super::view_model::InteractiveState;
use super::super::terminal_flow::{
    confirm, confirm_workflow_action, consume_outcome, outcome_notice, outcome_was_dispatched,
    terminal_fault_error, write_pre_dispatch_frame,
};
use super::super::TuiRuntimePort;
use super::LoopControl;

pub(super) fn dispatch_workflow(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    words: &[&str],
) -> Result<Option<LoopControl>, AppError> {
    let control = match words {
        ["select", selected_id] => {
            state.selected_id = Some((*selected_id).to_string());
            state.notice = format!("선택: {selected_id}");
            LoopControl::Continue
        }
        ["approve", proposal_id] => approve_patch(terminal, runtime, state, proposal_id)?,
        ["approve", "verification", proposal_id] => {
            approve_verification(terminal, runtime, state, proposal_id)?
        }
        [action @ ("deny" | "resume" | "cancel")] => {
            dispatch_workflow_action(terminal, runtime, state, action)?
        }
        _ => return Ok(None),
    };
    Ok(Some(control))
}

fn approve_patch(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    proposal_id: &str,
) -> Result<LoopControl, AppError> {
    let Some(workflow_id) = selected_workflow(state) else {
        return Ok(LoopControl::Continue);
    };
    if !cfg!(unix) {
        state.notice = outcome_notice(exact_tui_outcome(
            TuiOutcomeCode::SourceInstallUnsupportedPlatform,
            TuiOutcomeContext {
                platform: Some(std::env::consts::OS),
                ..TuiOutcomeContext::default()
            },
        )?);
        return Ok(LoopControl::Continue);
    }
    if !confirm(
        terminal,
        "패치 적용 확인",
        "패치 적용 승인",
        format!("proposal {proposal_id}을 검증한 뒤 선택한 workflow에 적용"),
    )? {
        state.notice = "승인을 보내지 않았습니다.".to_string();
        return Ok(LoopControl::Continue);
    }
    let intent_id = runtime.new_tui_intent_id();
    let lease = runtime.tui_selection_lease(&workflow_id)?;
    write_pre_dispatch_frame(terminal, &intent_id, "토큰을 무반향으로 입력하세요.\n")?;
    let Some(secret) = terminal.read_secret().map_err(terminal_fault_error)? else {
        state.notice = "비밀 입력 EOF: 승인을 보내지 않았습니다.".to_string();
        return Ok(LoopControl::Continue);
    };
    let outcome = runtime.dispatch_tui_intent(TuiIntent::ApprovePatch {
        intent_id: intent_id.clone(),
        proposal_id: proposal_id.to_string(),
        lease,
        secret: OneShotSecret::new(secret)?,
    })?;
    let consumed = consume_outcome(terminal, &intent_id, outcome)?;
    state.notice = consumed.notice;
    Ok(post_dispatch(intent_id, consumed.was_dispatched))
}

fn approve_verification(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    proposal_id: &str,
) -> Result<LoopControl, AppError> {
    let Some(workflow_id) = selected_workflow(state) else {
        return Ok(LoopControl::Continue);
    };
    if !confirm(
        terminal,
        "검증 실행 확인",
        "검증 실행 승인",
        format!("proposal {proposal_id}의 검증 단계를 실행"),
    )? {
        state.notice = "검증 승인을 보내지 않았습니다.".to_string();
        return Ok(LoopControl::Continue);
    }
    let intent_id = runtime.new_tui_intent_id();
    let lease = runtime.tui_selection_lease(&workflow_id)?;
    write_pre_dispatch_frame(terminal, &intent_id, "토큰을 무반향으로 입력하세요.\n")?;
    let Some(secret) = terminal.read_secret().map_err(terminal_fault_error)? else {
        state.notice = "비밀 입력 EOF: 검증 승인을 보내지 않았습니다.".to_string();
        return Ok(LoopControl::Continue);
    };
    let outcome = runtime.dispatch_tui_intent(TuiIntent::ApproveVerification {
        intent_id: intent_id.clone(),
        proposal_id: proposal_id.to_string(),
        lease,
        secret: OneShotSecret::new(secret)?,
    })?;
    let was_dispatched = outcome_was_dispatched(outcome.effect);
    state.notice = outcome_notice(outcome);
    Ok(post_dispatch(intent_id, was_dispatched))
}

fn dispatch_workflow_action(
    terminal: &mut impl TerminalIo,
    runtime: &mut impl TuiRuntimePort,
    state: &mut InteractiveState,
    action: &str,
) -> Result<LoopControl, AppError> {
    let Some(workflow_id) = selected_workflow(state) else {
        return Ok(LoopControl::Continue);
    };
    if !confirm_workflow_action(terminal, action, &workflow_id)? {
        state.notice = "요청을 보내지 않았습니다.".to_string();
        return Ok(LoopControl::Continue);
    }
    let intent_id = runtime.new_tui_intent_id();
    let gate = (action == "deny")
        .then(|| runtime.tui_gate_descriptor(&workflow_id))
        .transpose()?;
    let lease = runtime.tui_selection_lease(&workflow_id)?;
    write_pre_dispatch_frame(terminal, &intent_id, "정본 상태를 재검증했습니다.\n")?;
    let intent = match action {
        "deny" => TuiIntent::DenyPendingGate {
            intent_id: intent_id.clone(),
            workflow_id,
            gate_id: gate.as_ref().expect("deny gate prepared").0.clone(),
            gate_kind: gate.expect("deny gate prepared").1,
            lease,
        },
        "resume" => TuiIntent::ResumeWorkflow {
            intent_id: intent_id.clone(),
            workflow_id,
            lease,
        },
        "cancel" => TuiIntent::CancelWorkflow {
            intent_id: intent_id.clone(),
            workflow_id,
            lease,
        },
        _ => unreachable!("dispatch_workflow_action only receives admitted actions"),
    };
    let outcome = runtime.dispatch_tui_intent(intent)?;
    let was_dispatched = outcome_was_dispatched(outcome.effect);
    state.notice = outcome_notice(outcome);
    Ok(post_dispatch(intent_id, was_dispatched))
}

fn selected_workflow(state: &mut InteractiveState) -> Option<String> {
    let Some(workflow_id) = state.selected_id.clone() else {
        state.notice = "먼저 select <workflow-id>를 실행하세요.".to_string();
        return None;
    };
    Some(workflow_id)
}

fn post_dispatch(intent_id: String, was_dispatched: bool) -> LoopControl {
    if was_dispatched {
        LoopControl::PostDispatch(intent_id)
    } else {
        LoopControl::Continue
    }
}
