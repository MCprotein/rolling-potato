use std::collections::BTreeMap;

use crate::foundation::error::AppError;
use crate::runtime_core::knowledge::evidence::EvidenceStoreStatus;
use crate::runtime_core::observability::facade::{MonitorProjectionSnapshot, StoreStatus};
use crate::runtime_core::patch::proposal::PatchProposalDetail;
use crate::runtime_core::workflow::domain::snapshot::TuiStateSnapshot;
use crate::runtime_core::workflow::domain::transcript::ToolOutputView;
use crate::runtime_core::workflow::storage_compat::ledger::ParsedLedgerEvent;
use crate::runtime_core::workflow::storage_compat::record::WorkflowRecord;
use crate::runtime_core::workflow::storage_compat::transcript::TranscriptRecord;
use crate::surfaces::tui::outcome::validate_tui_id;
use crate::surfaces::tui::page::{
    bounded_budget_for, build_page, page_continuation, page_has_next, page_meta, page_slice,
    paged_chars, paged_diff, state_page_authority, tui_read_freshness as page_freshness,
    unavailable_page, ProjectionStatus,
};
use crate::surfaces::tui::runtime_bridge::{
    TuiFreshness, TuiReadContinuation, TuiReadPage, TuiReadRequest,
};

pub(crate) trait TuiReadPort {
    fn state_snapshot(&mut self, max_ledger_events: usize) -> Result<TuiStateSnapshot, AppError>;
    fn store_status(&mut self) -> Result<StoreStatus, AppError>;
    fn monitor_snapshot(&mut self, limit: usize) -> Result<MonitorProjectionSnapshot, AppError>;
    fn transcript_record(
        &mut self,
        event: &ParsedLedgerEvent,
    ) -> Result<TranscriptRecord, AppError>;
    fn tool_output_view(
        &mut self,
        record: &TranscriptRecord,
        artifact_id: &str,
    ) -> Result<ToolOutputView, AppError>;
    fn proposal_detail(
        &mut self,
        workflow: &WorkflowRecord,
        proposal_id: &str,
        max_bytes: usize,
    ) -> Result<PatchProposalDetail, AppError>;
    fn evidence_status(
        &mut self,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<EvidenceStoreStatus, AppError>;
    fn content_hash(&mut self, value: &str) -> String;
    fn projection_status(&mut self, project_id: &str) -> ProjectionStatus;
}

pub(crate) fn read_tui_page(
    port: &mut impl TuiReadPort,
    request: TuiReadRequest,
) -> Result<TuiReadPage, AppError> {
    match request {
        TuiReadRequest::Overview { budget } => {
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
            let freshness = tui_read_freshness(
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
        TuiReadRequest::Monitor { budget } => {
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
                    tui_read_freshness(
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
        TuiReadRequest::Sessions { page, budget } => {
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
                    tui_read_freshness(
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
        TuiReadRequest::Transcript {
            session_id,
            page,
            budget,
        } => {
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
                    tui_read_freshness(
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
        TuiReadRequest::ToolOutput {
            artifact_id,
            page,
            budget,
        } => {
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
                    tui_read_freshness(
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
        TuiReadRequest::Approvals { page, budget } => {
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
                let detail =
                    port.proposal_detail(workflow, &workflow.proposal_id, 2 * 1024 * 1024)?;
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
                    tui_read_freshness(
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
        TuiReadRequest::Diff {
            proposal_id,
            page,
            budget,
        } => {
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
            let (text, has_next) =
                paged_diff(&detail.diff, page, budget.max_items, budget.max_chars);
            let projected_events = port.store_status().ok().map(|store| store.ledger_events);
            let mut authority = state_page_authority(&snapshot, projected_events);
            authority.content_hash = Some(port.content_hash(&detail.diff));
            Ok(build_page(
                "diff",
                vec![
                    format!(
                        "proposal {} | path={} | status={}",
                        detail.summary.proposal_id,
                        detail.summary.relative_path,
                        detail.summary.status
                    ),
                    text,
                ],
                budget,
                page_meta(
                    page,
                    has_next,
                    tui_read_freshness(
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
        TuiReadRequest::Evidence { page, budget } => {
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
                    tui_read_freshness(
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
    }
}

fn optional_metric(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "unavailable".to_string())
}

fn tui_read_freshness(
    port: &mut impl TuiReadPort,
    project_id: &str,
    canonical_events: u64,
    projected_events: Option<i64>,
) -> TuiFreshness {
    let projection_status = port.projection_status(project_id);
    page_freshness(canonical_events, projected_events, projection_status)
}
