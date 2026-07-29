use crate::foundation::error::AppError;
use crate::surfaces::tui::outcome::validate_tui_id;
use crate::surfaces::tui::page::{
    bounded_budget_for, build_page, page_continuation, page_has_next, page_meta, page_slice,
    paged_chars, state_page_authority, unavailable_page,
};
use crate::surfaces::tui::runtime_bridge::{TuiReadBudget, TuiReadContinuation, TuiReadPage};

use super::common::freshness;
use super::TuiReadPort;

pub(super) fn transcript(
    port: &mut impl TuiReadPort,
    session_id: String,
    page: u64,
    budget: TuiReadBudget,
) -> Result<TuiReadPage, AppError> {
    let budget = bounded_budget_for(budget, 50, 48 * 1024);
    validate_tui_id(&session_id, "session")?;
    let snapshot = port.state_snapshot(200)?;
    let mut rows = snapshot
        .ledger_events
        .iter()
        .filter(|event| {
            event.project_id == snapshot.identity.project_id
                && event.session_id == session_id
                && event.event_type == "transcript.recorded"
        })
        .map(|event| port.transcript_record(event))
        .collect::<Result<Vec<_>, _>>()?;
    rows.sort_by_key(|record| (record.recorded_at_ms, record.record_id.clone()));
    let total = rows.len();
    let selected = page_slice(rows, page, budget.max_items);
    let transcript_hash = selected.last().map(|record| record.artifact_hash.clone());
    let lines = selected
        .into_iter()
        .map(|record| {
            format!(
                "{} | kind={} | workflow={} | recorded-at={} | {}",
                record.record_id,
                record.kind,
                record.workflow_id,
                record.recorded_at_ms,
                record.content
            )
        })
        .collect();
    let has_next = page_has_next(page, budget.max_items, total);
    let projected_events = port.store_status().ok().map(|store| store.ledger_events);
    let mut authority = state_page_authority(&snapshot, projected_events);
    authority.transcript_hash = transcript_hash;
    Ok(build_page(
        "transcript",
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
            authority,
            page_continuation(has_next, snapshot.ledger_tail_truncated),
        ),
    ))
}

pub(super) fn tool_output(
    port: &mut impl TuiReadPort,
    artifact_id: String,
    page: u64,
    budget: TuiReadBudget,
) -> Result<TuiReadPage, AppError> {
    let budget = bounded_budget_for(budget, 16, 64 * 1024);
    validate_tui_id(&artifact_id, "tool artifact")?;
    let snapshot = port.state_snapshot(64)?;
    let mut matched = None;
    for event in snapshot.ledger_events.iter().rev() {
        if event.project_id != snapshot.identity.project_id
            || event.event_type != "transcript.recorded"
        {
            continue;
        }
        let record = port.transcript_record(event)?;
        if record
            .tool_output_artifact
            .as_ref()
            .is_some_and(|binding| binding.id == artifact_id)
        {
            matched = Some(record);
            break;
        }
    }
    let Some(record) = matched else {
        return Ok(unavailable_page(
            "tool-output",
            page,
            budget,
            "canonical transcript ledger binding이 bounded scan 안에 없습니다.",
            state_page_authority(&snapshot, None),
            snapshot.ledger_tail_truncated,
        ));
    };
    let artifact = port.tool_output_view(&record, &artifact_id)?;
    let body = format!(
        "artifact: {}\nsession: {}\nworkflow: {}\ntool: {}\nrecorded-at: {}\nstdout-truncated: {} redacted: {}\nstderr-truncated: {} redacted: {}\n[stdout]\n{}\n[stderr]\n{}",
        artifact.artifact_id,
        artifact.session_id,
        artifact.workflow_id,
        artifact.tool_id,
        artifact.created_at_ms,
        artifact.stdout_truncated,
        artifact.stdout_redacted,
        artifact.stderr_truncated,
        artifact.stderr_redacted,
        artifact.stdout,
        artifact.stderr,
    );
    let (text, has_next) = paged_chars(&body, page, budget.max_chars);
    let projected_events = port.store_status().ok().map(|store| store.ledger_events);
    let mut authority = state_page_authority(&snapshot, projected_events);
    authority.content_hash = record
        .tool_output_artifact
        .as_ref()
        .map(|binding| binding.hash.clone());
    authority.transcript_hash = Some(record.artifact_hash);
    let continuation = if has_next {
        TuiReadContinuation::NextPage
    } else if artifact.stdout_redacted || artifact.stderr_redacted {
        TuiReadContinuation::Redacted
    } else {
        TuiReadContinuation::Complete
    };
    Ok(build_page(
        "tool-output",
        vec![text],
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
            continuation,
        ),
    ))
}
