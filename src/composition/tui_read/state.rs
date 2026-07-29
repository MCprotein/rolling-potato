use std::collections::BTreeMap;

use crate::foundation::error::AppError;
use crate::surfaces::tui::page::{
    bounded_budget_for, build_page, page_continuation, page_has_next, page_meta, page_slice,
    state_page_authority,
};
use crate::surfaces::tui::runtime_bridge::{TuiReadBudget, TuiReadContinuation, TuiReadPage};

use super::common::{freshness, optional_metric};
use super::TuiReadPort;

pub(super) fn overview(
    port: &mut impl TuiReadPort,
    budget: TuiReadBudget,
) -> Result<TuiReadPage, AppError> {
    let budget = bounded_budget_for(budget, 20, 24 * 1024);
    let snapshot = port.state_snapshot(80)?;
    let store = port.store_status().ok();
    let projected_events = store.as_ref().map(|store| store.ledger_events);
    let mut lines = vec![
        format!("project: {}", snapshot.identity.project_root),
        format!("session: {}", snapshot.identity.session_id),
        format!(
            "current: revision={} hash={}",
            snapshot.current_revision, snapshot.current_hash
        ),
        format!(
            "ledger: sequence={} hash={}",
            snapshot.ledger_binding.event_count, snapshot.ledger_binding.event_hash
        ),
        format!(
            "canonical scan: truncated={} current-binding-stale={}",
            snapshot.ledger_tail_truncated, snapshot.current_ledger_binding_stale
        ),
    ];
    if let Some(store) = store.as_ref() {
        lines.extend([
            format!("projected ledger events: {}", store.ledger_events),
            format!("projected sessions: {}", store.sessions),
            format!("projected workflows: {}", store.workflows),
            format!("projected transcript records: {}", store.transcript_records),
        ]);
    } else {
        lines.push("observability projection: unavailable".to_string());
    }
    if let Some(workflow) = snapshot.active_workflow.as_ref() {
        lines.push(format!("active workflow: {}", workflow.workflow_id));
        lines.push(format!("workflow phase: {}", workflow.phase));
        lines.push(format!(
            "workflow: revision={} hash={}",
            workflow.revision, workflow.artifact_hash
        ));
    } else {
        lines.push("active workflow: none".to_string());
    }
    let freshness = freshness(
        port,
        &snapshot.identity.project_id,
        snapshot.ledger_binding.event_count,
        projected_events,
    );
    Ok(build_page(
        "overview",
        lines,
        budget,
        page_meta(
            0,
            false,
            freshness,
            state_page_authority(&snapshot, projected_events),
            if snapshot.ledger_tail_truncated {
                TuiReadContinuation::Truncated
            } else {
                TuiReadContinuation::Complete
            },
        ),
    ))
}

pub(super) fn monitor(
    port: &mut impl TuiReadPort,
    budget: TuiReadBudget,
) -> Result<TuiReadPage, AppError> {
    let budget = bounded_budget_for(budget, 120, 48 * 1024);
    let snapshot = port.state_snapshot(480)?;
    let projection = port.monitor_snapshot(budget.max_items).ok();
    let projected_events = projection
        .as_ref()
        .map(|projection| projection.status.ledger_events);
    let mut lines = match projection.as_ref() {
        Some(projection) => vec![
            format!("database: {}", projection.status.path.display()),
            format!("model runs: {}", projection.status.model_runs),
            format!("token records: {}", projection.status.token_records),
            format!("resource samples: {}", projection.status.resource_samples),
            format!("benchmark runs: {}", projection.status.benchmark_runs),
        ],
        None => vec!["observability projection: unavailable".to_string()],
    };
    for model in projection
        .map(|projection| projection.model_summaries)
        .unwrap_or_default()
        .into_iter()
        .take(budget.max_items.saturating_sub(lines.len()))
    {
        lines.push(format!(
            "model {}: runs={} tokens={} avg_latency_ms={} avg_tps={}",
            model.model_id,
            model.runs,
            model.total_tokens,
            optional_metric(model.avg_latency_ms),
            optional_metric(model.avg_tokens_per_second)
        ));
    }
    Ok(build_page(
        "monitor",
        lines,
        budget,
        page_meta(
            0,
            false,
            freshness(
                port,
                &snapshot.identity.project_id,
                snapshot.ledger_binding.event_count,
                projected_events,
            ),
            state_page_authority(&snapshot, projected_events),
            TuiReadContinuation::Complete,
        ),
    ))
}

pub(super) fn sessions(
    port: &mut impl TuiReadPort,
    page: u64,
    budget: TuiReadBudget,
) -> Result<TuiReadPage, AppError> {
    let budget = bounded_budget_for(budget, 50, 32 * 1024);
    let snapshot = port.state_snapshot(200)?;
    let mut sessions = BTreeMap::<String, (usize, u128, String)>::new();
    for event in &snapshot.ledger_events {
        if event.project_id != snapshot.identity.project_id {
            continue;
        }
        let entry = sessions.entry(event.session_id.clone()).or_insert((
            0,
            event.ts_ms,
            event.summary.clone(),
        ));
        entry.0 = entry.0.saturating_add(1);
        if event.ts_ms >= entry.1 {
            entry.1 = event.ts_ms;
            entry.2.clone_from(&event.summary);
        }
    }
    let mut rows = sessions.into_iter().collect::<Vec<_>>();
    rows.sort_by_key(|(_, (_, ts, _))| std::cmp::Reverse(*ts));
    let total = rows.len();
    let lines = page_slice(rows, page, budget.max_items)
        .into_iter()
        .map(|(session_id, (tail_events, recorded_at, summary))| {
            let selected = if session_id == snapshot.identity.session_id {
                " selected"
            } else {
                ""
            };
            format!(
                "{}{} | canonical-tail-events={} | last={} | recorded-at={}",
                session_id, selected, tail_events, summary, recorded_at
            )
        })
        .collect();
    let has_next = page_has_next(page, budget.max_items, total);
    let projected_events = port.store_status().ok().map(|store| store.ledger_events);
    Ok(build_page(
        "sessions",
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
