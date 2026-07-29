use crate::foundation::error::AppError;
use crate::surfaces::tui::page::{
    bounded_budget_for, build_page, page_continuation, page_has_next, page_meta, page_slice,
    paged_diff, state_page_authority, unavailable_page,
};
use crate::surfaces::tui::runtime_bridge::{TuiReadBudget, TuiReadContinuation, TuiReadPage};

use super::common::freshness;
use super::TuiReadPort;

pub(super) fn approvals(
    port: &mut impl TuiReadPort,
    page: u64,
    budget: TuiReadBudget,
) -> Result<TuiReadPage, AppError> {
    let budget = bounded_budget_for(budget, 20, 24 * 1024);
    let snapshot = port.state_snapshot(80)?;
    let mut lines = snapshot
        .ledger_events
        .iter()
        .filter(|event| event.project_id == snapshot.identity.project_id)
        .filter_map(|event| {
            let status = match event.event_type.as_str() {
                "team.admission.policy_blocked" => "pending-approval",
                "team.admission.ownership_blocked" | "team.admission.blocked" => "blocked",
                _ => return None,
            };
            Some(format!(
                "request team-{} | status={} | source=team-admission | canonical-event={}",
                event.event_id, status, event.event_id
            ))
        })
        .collect::<Vec<_>>();
    if let Some(workflow) = snapshot
        .active_workflow
        .as_ref()
        .filter(|workflow| !workflow.proposal_id.is_empty())
    {
        let detail = port.proposal_detail(workflow, &workflow.proposal_id, 2 * 1024 * 1024)?;
        lines.push(format!(
            "proposal {} | status={} | path={} | {} -> {}",
            detail.summary.proposal_id,
            detail.summary.status,
            detail.summary.relative_path,
            detail.summary.original_sha256,
            detail.summary.proposed_sha256
        ));
    }
    let total = lines.len();
    let lines = page_slice(lines, page, budget.max_items);
    let has_next = page_has_next(page, budget.max_items, total);
    let projected_events = port.store_status().ok().map(|store| store.ledger_events);
    Ok(build_page(
        "approvals",
        lines,
        budget,
        page_meta(
            page,
            has_next,
            freshness(
                port,
                &snapshot.identity.project_id,
                snapshot.ledger_binding.event_count,
                projected_events,
            ),
            state_page_authority(&snapshot, projected_events),
            page_continuation(has_next, snapshot.ledger_tail_truncated),
        ),
    ))
}

pub(super) fn diff(
    port: &mut impl TuiReadPort,
    proposal_id: String,
    page: u64,
    budget: TuiReadBudget,
) -> Result<TuiReadPage, AppError> {
    let budget = bounded_budget_for(budget, 120, 64 * 1024);
    let snapshot = port.state_snapshot(80)?;
    let Some(workflow) = snapshot.active_workflow.as_ref() else {
        return Ok(unavailable_page(
            "diff",
            page,
            budget,
            "active workflow canonical binding이 없습니다.",
            state_page_authority(&snapshot, None),
            snapshot.ledger_tail_truncated,
        ));
    };
    if workflow.proposal_id != proposal_id {
        return Ok(unavailable_page(
            "diff",
            page,
            budget,
            "요청한 proposal이 active workflow에 bound되지 않았습니다.",
            state_page_authority(&snapshot, None),
            snapshot.ledger_tail_truncated,
        ));
    }
    let detail = port.proposal_detail(workflow, &proposal_id, 2 * 1024 * 1024)?;
    let (text, has_next) = paged_diff(&detail.diff, page, budget.max_items, budget.max_chars);
    let projected_events = port.store_status().ok().map(|store| store.ledger_events);
    let mut authority = state_page_authority(&snapshot, projected_events);
    authority.content_hash = Some(port.content_hash(&detail.diff));
    Ok(build_page(
        "diff",
        vec![
            format!(
                "proposal {} | path={} | status={}",
                detail.summary.proposal_id, detail.summary.relative_path, detail.summary.status
            ),
            text,
        ],
        budget,
        page_meta(
            page,
            has_next,
            freshness(
                port,
                &snapshot.identity.project_id,
                snapshot.ledger_binding.event_count,
                projected_events,
            ),
            authority,
            page_continuation(has_next, false),
        ),
    ))
}

pub(super) fn evidence(
    port: &mut impl TuiReadPort,
    page: u64,
    budget: TuiReadBudget,
) -> Result<TuiReadPage, AppError> {
    let budget = bounded_budget_for(budget, 25, 48 * 1024);
    let snapshot = port.state_snapshot(100)?;
    let status = port.evidence_status(100, 2 * 1024 * 1024)?;
    let projected_events = port.store_status().ok().map(|store| store.ledger_events);
    let mut authority = state_page_authority(&snapshot, projected_events);
    if let Some(workflow) = snapshot.active_workflow.as_ref() {
        authority.content_hash =
            (!workflow.evidence_hash.is_empty()).then(|| workflow.evidence_hash.clone());
    }
    Ok(build_page(
        "evidence",
        vec![
            format!("runtime file: {}", status.runtime_evidence_file.display()),
            format!("runtime records: {}", status.runtime_evidence_records),
            format!(
                "project directory: {}",
                status.project_evidence_dir.display()
            ),
            format!("project artifacts: {}", status.project_artifacts),
            format!("stale policy: {}", status.stale_policy),
        ],
        budget,
        page_meta(
            page,
            false,
            freshness(
                port,
                &snapshot.identity.project_id,
                snapshot.ledger_binding.event_count,
                projected_events,
            ),
            authority,
            if status.truncated {
                TuiReadContinuation::Truncated
            } else {
                TuiReadContinuation::Complete
            },
        ),
    ))
}
