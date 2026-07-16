use crate::app::AppError;
use crate::{approval, ledger, observability, paths, policy, resource, state, team_state};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

pub fn status_report() -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let store = observability::status()?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let decision = resource::team_lane_decision(pressure, resource::DEFAULT_TEAM_REQUESTED_LANES);
    let dispatch_blocked = if decision.is_blocked() { "yes" } else { "no" };
    let latest_team_event = latest_team_runtime_event(&identity)?;
    let active_parent = state::active_workflow_id()?;
    let latest_team_state = match active_parent.as_deref() {
        Some(parent_workflow_id) => team_state::latest_for_parent(parent_workflow_id)?,
        None => None,
    };

    Ok(format!(
        "team status\n- status: admission-preview\n- observability store: {}\n- resource samples: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- current team id: {}\n- current team stage: {}\n- current team status: {}\n- current team revision: {}\n- current team execution mode: {}\n- latest team runtime event: {}\n- latest team runtime summary: {}\n- latest team runtime event id: {}\n- reason: {}\n- hint: {}\n- boundary: read-only status only; does not start subagents, dispatch team lanes, mutate workflows, or bypass approval/file ownership policy.",
        store.path.display(),
        store.resource_samples,
        if sample.is_some() {
            "latest-resource-sample"
        } else {
            "no-sample"
        },
        sample
            .as_ref()
            .map(|sample| sample.resource_sample_id.as_str())
            .unwrap_or("없음"),
        sample
            .as_ref()
            .map(|sample| sample.recorded_at_ms.to_string())
            .unwrap_or_else(|| "없음".to_string()),
        decision.pressure.as_str(),
        display_optional_f64(sample.as_ref().and_then(|sample| sample.process_cpu_percent)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.average_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.peak_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.disk_bytes)),
        decision.requested_lanes,
        decision.admitted_lanes,
        decision.admission.as_str(),
        dispatch_blocked,
        decision.fallback,
        latest_team_state
            .as_ref()
            .map(|record| record.team_id.as_str())
            .unwrap_or("없음"),
        latest_team_state
            .as_ref()
            .map(|record| record.stage.as_str())
            .unwrap_or("없음"),
        latest_team_state
            .as_ref()
            .map(|record| record.status.as_str())
            .unwrap_or("없음"),
        latest_team_state
            .as_ref()
            .map(|record| record.revision.to_string())
            .unwrap_or_else(|| "없음".to_string()),
        latest_team_state
            .as_ref()
            .map(|record| record.execution_mode.as_str())
            .unwrap_or("없음"),
        latest_team_event
            .as_ref()
            .map(|event| event.event_type.as_str())
            .unwrap_or("없음"),
        latest_team_event
            .as_ref()
            .map(|event| event.summary.as_str())
            .unwrap_or("없음"),
        latest_team_event
            .as_ref()
            .map(|event| event.event_id.as_str())
            .unwrap_or("없음"),
        decision.reason,
        decision.hint
    ))
}

pub fn admission_report(
    requested_lanes: u32,
    write_paths: &[String],
    owned_write_paths: &[(u32, String)],
    commands: &[String],
) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let store = observability::initialize(&identity)?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let decision = resource::team_lane_decision(pressure, requested_lanes);
    let policy_write_paths = policy_write_paths(write_paths, owned_write_paths);
    let policy_gate = policy_preflight(&policy_write_paths, commands)?;
    let ownership_gate = ownership_preflight(decision.admitted_lanes, owned_write_paths)?;
    let blocked_by_resource = decision.is_blocked();
    let blocked_by_policy = policy_gate.is_blocked();
    let blocked_by_ownership = ownership_gate.is_blocked();
    let dispatch_blocked = if blocked_by_resource || blocked_by_policy || blocked_by_ownership {
        "yes"
    } else {
        "no"
    };
    let event_type =
        admission_event_type(decision.admission, blocked_by_policy, blocked_by_ownership);
    let event = ledger::new_event_for(
        &identity,
        event_type,
        admission_summary(decision.admission, blocked_by_policy, blocked_by_ownership),
        &format!(
            "requested_lanes={} admitted_lanes={} admission={} dispatch_blocked={} fallback={} pressure={} resource_sample_id={} policy_status={} policy_blocked={} ownership_status={} ownership_blocked={} write_paths={} owned_write_paths={} commands={} reason={}",
            decision.requested_lanes,
            decision.admitted_lanes,
            decision.admission.as_str(),
            dispatch_blocked,
            decision.fallback,
            decision.pressure.as_str(),
            sample
                .as_ref()
                .map(|sample| sample.resource_sample_id.as_str())
                .unwrap_or("none"),
            policy_gate.status,
            policy_gate.blocked_label(),
            ownership_gate.status,
            ownership_gate.blocked_label(),
            display_list(write_paths),
            display_owned_write_paths(owned_write_paths),
            display_redacted_list(commands),
            decision.reason
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)?;
    let approval_request = record_approval_request(
        &identity,
        &event,
        overall_status(decision.admission, blocked_by_policy, blocked_by_ownership),
        &policy_gate,
        &ownership_gate,
    )?;

    let report = format!(
        "team admission\n- status: {}\n- observability store: {}\n- session id: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- policy checks: {}\n- policy status: {}\n- policy blocked: {}\n- write paths: {}\n- commands: {}\n- policy decisions:\n{}\n- ownership claims: {}\n- ownership status: {}\n- ownership blocked: {}\n- owned write paths: {}\n- ownership decisions:\n{}\n- approval request: {}\n- approval request path: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- reason: {}\n- hint: {}\n- ledger event: {}\n- boundary: admission gate only; records the decision and does not start workers, mutate team stages, bypass approval policy, or write files.",
        overall_status(decision.admission, blocked_by_policy, blocked_by_ownership),
        store.path.display(),
        identity.session_id,
        decision.requested_lanes,
        decision.admitted_lanes,
        decision.admission.as_str(),
        dispatch_blocked,
        decision.fallback,
        policy_gate.checks.len(),
        policy_gate.status,
        policy_gate.blocked_label(),
        display_list(write_paths),
        display_redacted_list(commands),
        format_policy_checks(&policy_gate.checks),
        ownership_gate.checks.len(),
        ownership_gate.status,
        ownership_gate.blocked_label(),
        display_owned_write_paths(owned_write_paths),
        format_ownership_checks(&ownership_gate.checks),
        approval_request
            .as_ref()
            .map(|request| request.request_id.as_str())
            .unwrap_or("not-required"),
        approval_request
            .as_ref()
            .map(|request| request.path.display().to_string())
            .unwrap_or_else(|| "없음".to_string()),
        if sample.is_some() {
            "latest-resource-sample"
        } else {
            "no-sample"
        },
        sample
            .as_ref()
            .map(|sample| sample.resource_sample_id.as_str())
            .unwrap_or("없음"),
        sample
            .as_ref()
            .map(|sample| sample.recorded_at_ms.to_string())
            .unwrap_or_else(|| "없음".to_string()),
        decision.pressure.as_str(),
        display_optional_f64(sample.as_ref().and_then(|sample| sample.process_cpu_percent)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.average_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.peak_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.disk_bytes)),
        decision.reason,
        decision.hint,
        event.event_id
    );

    if blocked_by_resource || blocked_by_policy || blocked_by_ownership {
        return Err(AppError::blocked(format!(
            "team admission 차단\n{}",
            report
        )));
    }

    Ok(report)
}

pub fn dispatch_report(
    requested_lanes: u32,
    owned_write_paths: &[(u32, String)],
    failed_lane: Option<u32>,
    failure_reason: Option<&str>,
) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let store = observability::initialize(&identity)?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let lane_decision = resource::team_lane_decision(pressure, requested_lanes);
    let ownership_gate = ownership_preflight(lane_decision.admitted_lanes, owned_write_paths)?;
    let continuation = continuation_decision(
        lane_decision.admitted_lanes,
        failed_lane,
        failure_reason.unwrap_or("not-provided"),
    );
    let blocked_by_resource = lane_decision.is_blocked();
    let blocked_by_ownership = ownership_gate.is_blocked();
    let blocked_by_continuation = continuation.is_blocked();
    let dispatch_blocked = if blocked_by_resource || blocked_by_ownership || blocked_by_continuation
    {
        "yes"
    } else {
        "no"
    };
    let status = dispatch_status(lane_decision.admission, blocked_by_ownership, &continuation);
    let event = ledger::new_event_for(
        &identity,
        dispatch_event_type(
            lane_decision.admission,
            blocked_by_ownership,
            &continuation,
        ),
        dispatch_summary(
            lane_decision.admission,
            blocked_by_ownership,
            &continuation,
        ),
        &format!(
            "requested_lanes={} admitted_lanes={} admission={} dispatch_blocked={} fallback={} pressure={} resource_sample_id={} ownership_status={} ownership_blocked={} owned_write_paths={} failed_lane={} failure_reason={} continuation_status={} continuation_action={} continuation_remaining_lanes={} reason={}",
            lane_decision.requested_lanes,
            lane_decision.admitted_lanes,
            lane_decision.admission.as_str(),
            dispatch_blocked,
            lane_decision.fallback,
            lane_decision.pressure.as_str(),
            sample
                .as_ref()
                .map(|sample| sample.resource_sample_id.as_str())
                .unwrap_or("none"),
            ownership_gate.status,
            ownership_gate.blocked_label(),
            display_owned_write_paths(owned_write_paths),
            display_optional_lane(failed_lane),
            ledger::redact_text(failure_reason.unwrap_or("not-provided")),
            continuation.status,
            continuation.action,
            continuation.remaining_lanes,
            continuation.reason
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)?;

    let report = format!(
        "team dispatch\n- status: {}\n- observability store: {}\n- session id: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- lane admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- ownership claims: {}\n- ownership status: {}\n- ownership blocked: {}\n- owned write paths: {}\n- ownership decisions:\n{}\n- failed lane: {}\n- failure reason: {}\n- continuation status: {}\n- continuation action: {}\n- continuation remaining lanes: {}\n- continuation reason: {}\n- continuation hint: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- reason: {}\n- hint: {}\n- ledger event: {}\n- boundary: dispatch preflight only; records ownership and failed-worker continuation state, but does not start subagents, execute tools, merge files, or advance team stages.",
        status,
        store.path.display(),
        identity.session_id,
        lane_decision.requested_lanes,
        lane_decision.admitted_lanes,
        lane_decision.admission.as_str(),
        dispatch_blocked,
        lane_decision.fallback,
        ownership_gate.checks.len(),
        ownership_gate.status,
        ownership_gate.blocked_label(),
        display_owned_write_paths(owned_write_paths),
        format_ownership_checks(&ownership_gate.checks),
        display_optional_lane(failed_lane),
        ledger::redact_text(failure_reason.unwrap_or("not-provided")),
        continuation.status,
        continuation.action,
        continuation.remaining_lanes,
        continuation.reason,
        continuation.hint,
        if sample.is_some() {
            "latest-resource-sample"
        } else {
            "no-sample"
        },
        sample
            .as_ref()
            .map(|sample| sample.resource_sample_id.as_str())
            .unwrap_or("없음"),
        sample
            .as_ref()
            .map(|sample| sample.recorded_at_ms.to_string())
            .unwrap_or_else(|| "없음".to_string()),
        lane_decision.pressure.as_str(),
        display_optional_f64(sample.as_ref().and_then(|sample| sample.process_cpu_percent)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.average_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.peak_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.disk_bytes)),
        lane_decision.reason,
        lane_decision.hint,
        event.event_id
    );

    if blocked_by_resource || blocked_by_ownership || blocked_by_continuation {
        return Err(AppError::blocked(format!("team dispatch 차단\n{}", report)));
    }

    Ok(report)
}

pub fn governor_report(
    requested_lanes: u32,
    requested_context_tokens: u32,
    context_limit_tokens: Option<u32>,
    model_tier: resource::ModelTier,
) -> Result<String, AppError> {
    let identity = ledger::validated_current_identity()?;
    let store = observability::initialize(&identity)?;
    let sample = observability::latest_resource_sample()?;
    let pressure = sample
        .as_ref()
        .map(|sample| pressure_from_status(&sample.pressure_status))
        .unwrap_or(resource::ResourcePressure::Unknown);
    let lane_decision = resource::team_lane_decision(pressure, requested_lanes);
    let context_limit_tokens =
        context_limit_tokens.unwrap_or(resource::DEFAULT_CONTEXT_LIMIT_TOKENS);
    let context_decision = resource::context_model_governor_decision(
        pressure,
        requested_context_tokens,
        context_limit_tokens,
        model_tier,
    );
    let dispatch_blocked = if lane_decision.is_blocked() || context_decision.is_blocked() {
        "yes"
    } else {
        "no"
    };
    let status = governor_status(&context_decision, &lane_decision);
    let event = ledger::new_event_for(
        &identity,
        governor_event_type(status),
        governor_summary(status),
        &format!(
            "requested_lanes={} admitted_lanes={} lane_admission={} dispatch_blocked={} fallback={} pressure={} resource_sample_id={} requested_context_tokens={} context_limit_tokens={} effective_context_tokens={} context_action={} model_tier={} model_hint={} reason={}",
            lane_decision.requested_lanes,
            lane_decision.admitted_lanes,
            lane_decision.admission.as_str(),
            dispatch_blocked,
            lane_decision.fallback,
            pressure.as_str(),
            sample
                .as_ref()
                .map(|sample| sample.resource_sample_id.as_str())
                .unwrap_or("none"),
            context_decision.requested_context_tokens,
            context_decision.context_limit_tokens,
            display_optional_u32(context_decision.effective_context_tokens),
            context_decision.context_action.as_str(),
            context_decision.model_tier.as_str(),
            context_decision.model_hint.as_str(),
            context_decision.reason
        ),
    );
    ledger::append_event(&event)?;
    observability::project_event(&event)?;

    let report = format!(
        "team governor\n- status: {}\n- observability store: {}\n- session id: {}\n- requested parallel lanes: {}\n- admitted lanes: {}\n- lane admission: {}\n- dispatch blocked: {}\n- fallback: {}\n- requested context tokens: {}\n- context limit tokens: {}\n- effective context tokens: {}\n- context action: {}\n- model tier: {}\n- model route hint: {}\n- resource sample source: {}\n- resource sample id: {}\n- resource recorded ms: {}\n- resource pressure: {}\n- resource cpu percent: {}\n- resource average rss bytes: {}\n- resource peak rss bytes: {}\n- resource disk bytes: {}\n- reason: {}\n- hint: {}\n- ledger event: {}\n- boundary: governor preflight only; records context/model admission hints and does not start workers, select real model artifacts, mutate team stages, or execute tools.",
        status,
        store.path.display(),
        identity.session_id,
        lane_decision.requested_lanes,
        lane_decision.admitted_lanes,
        lane_decision.admission.as_str(),
        dispatch_blocked,
        lane_decision.fallback,
        context_decision.requested_context_tokens,
        context_decision.context_limit_tokens,
        display_optional_u32(context_decision.effective_context_tokens),
        context_decision.context_action.as_str(),
        context_decision.model_tier.as_str(),
        context_decision.model_hint.as_str(),
        if sample.is_some() {
            "latest-resource-sample"
        } else {
            "no-sample"
        },
        sample
            .as_ref()
            .map(|sample| sample.resource_sample_id.as_str())
            .unwrap_or("없음"),
        sample
            .as_ref()
            .map(|sample| sample.recorded_at_ms.to_string())
            .unwrap_or_else(|| "없음".to_string()),
        context_decision.pressure.as_str(),
        display_optional_f64(sample.as_ref().and_then(|sample| sample.process_cpu_percent)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.average_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.peak_rss_bytes)),
        display_optional_u64(sample.as_ref().and_then(|sample| sample.disk_bytes)),
        context_decision.reason,
        context_decision.hint,
        event.event_id
    );

    if lane_decision.is_blocked() || context_decision.is_blocked() {
        return Err(AppError::blocked(format!("team governor 차단\n{}", report)));
    }

    Ok(report)
}

fn pressure_from_status(value: &str) -> resource::ResourcePressure {
    match value {
        "normal" => resource::ResourcePressure::Normal,
        "degraded" => resource::ResourcePressure::Degraded,
        "critical" => resource::ResourcePressure::Critical,
        _ => resource::ResourcePressure::Unknown,
    }
}

fn latest_team_runtime_event(
    identity: &ledger::RuntimeIdentity,
) -> Result<Option<ledger::ParsedLedgerEvent>, AppError> {
    let mut events = ledger::read_runtime_events()?;
    events.retain(|event| {
        event.project_id == identity.project_id && is_team_runtime_event(&event.event_type)
    });
    events.sort_by(|left, right| {
        left.ts_ms
            .cmp(&right.ts_ms)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    Ok(events.pop())
}

fn is_team_runtime_event(event_type: &str) -> bool {
    event_type.starts_with("team.admission.")
        || event_type.starts_with("team.dispatch.")
        || event_type.starts_with("team.continuation.")
        || event_type.starts_with("team.governor.")
        || event_type.starts_with("team.worker.")
        || event_type.starts_with("team.subagent.")
}

fn governor_status(
    context_decision: &resource::ContextModelGovernorDecision,
    lane_decision: &resource::ResourceLaneDecision,
) -> &'static str {
    if context_decision.is_blocked() || lane_decision.is_blocked() {
        "blocked"
    } else if context_decision.context_action == resource::ContextGovernorAction::Clamped {
        "clamped"
    } else if context_decision.model_hint != resource::ModelRouteHint::Keep {
        "hinted"
    } else {
        "allowed"
    }
}

fn governor_event_type(status: &str) -> &'static str {
    match status {
        "blocked" => "team.governor.blocked",
        "clamped" => "team.governor.clamped",
        "hinted" => "team.governor.hinted",
        _ => "team.governor.allowed",
    }
}

fn governor_summary(status: &str) -> &'static str {
    match status {
        "blocked" => "team governor blocked",
        "clamped" => "team governor context clamped",
        "hinted" => "team governor model route hinted",
        _ => "team governor allowed",
    }
}

fn admission_status(admission: resource::ResourceLaneAdmission) -> &'static str {
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "admitted",
        resource::ResourceLaneAdmission::SequentialFallback => "sequential-fallback",
        resource::ResourceLaneAdmission::Blocked => "blocked",
    }
}

fn overall_status(
    admission: resource::ResourceLaneAdmission,
    blocked_by_policy: bool,
    blocked_by_ownership: bool,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "blocked";
    }
    if blocked_by_ownership {
        return "ownership-blocked";
    }
    if blocked_by_policy {
        return "policy-blocked";
    }
    admission_status(admission)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ContinuationDecision {
    status: &'static str,
    action: &'static str,
    remaining_lanes: u32,
    reason: String,
    hint: &'static str,
}

impl ContinuationDecision {
    fn is_blocked(&self) -> bool {
        self.status == "blocked"
    }
}

fn continuation_decision(
    admitted_lanes: u32,
    failed_lane: Option<u32>,
    failure_reason: &str,
) -> ContinuationDecision {
    let Some(failed_lane) = failed_lane else {
        return ContinuationDecision {
            status: "not-requested",
            action: "none",
            remaining_lanes: admitted_lanes,
            reason: "no failed worker reported".to_string(),
            hint: "dispatch may proceed without continuation handling if other gates allow it",
        };
    };

    if failed_lane == 0 || failed_lane > admitted_lanes {
        return ContinuationDecision {
            status: "blocked",
            action: "none",
            remaining_lanes: 0,
            reason: format!(
                "failed lane {failed_lane} is outside admitted lanes {admitted_lanes}; cannot continue safely"
            ),
            hint: "re-run admission with current resources and a valid failed lane before continuing",
        };
    }

    if admitted_lanes <= 1 {
        return ContinuationDecision {
            status: "blocked",
            action: "wait",
            remaining_lanes: 0,
            reason: "no remaining admitted lanes after the failed worker".to_string(),
            hint: "resume as a single-agent repair or re-run admission after resources recover",
        };
    }

    ContinuationDecision {
        status: "continue-with-remaining",
        action: "continue",
        remaining_lanes: admitted_lanes - 1,
        reason: format!(
            "lane {failed_lane} is excluded after failure; reason recorded as {}",
            ledger::redact_text(failure_reason)
        ),
        hint: "continue only unfailed lanes and keep file ownership boundaries unchanged",
    }
}

fn dispatch_status(
    admission: resource::ResourceLaneAdmission,
    blocked_by_ownership: bool,
    continuation: &ContinuationDecision,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked || continuation.is_blocked() {
        return "blocked";
    }
    if blocked_by_ownership {
        return "ownership-blocked";
    }
    if continuation.status == "continue-with-remaining" {
        return "continuation-ready";
    }
    admission_status(admission)
}

fn dispatch_event_type(
    admission: resource::ResourceLaneAdmission,
    blocked_by_ownership: bool,
    continuation: &ContinuationDecision,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "team.dispatch.blocked";
    }
    if blocked_by_ownership {
        return "team.dispatch.ownership_blocked";
    }
    if continuation.is_blocked() {
        return "team.continuation.blocked";
    }
    if continuation.status == "continue-with-remaining" {
        return "team.continuation.recorded";
    }
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "team.dispatch.ready",
        resource::ResourceLaneAdmission::SequentialFallback => "team.dispatch.fallback",
        resource::ResourceLaneAdmission::Blocked => "team.dispatch.blocked",
    }
}

fn dispatch_summary(
    admission: resource::ResourceLaneAdmission,
    blocked_by_ownership: bool,
    continuation: &ContinuationDecision,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "team dispatch blocked";
    }
    if blocked_by_ownership {
        return "team dispatch ownership blocked";
    }
    if continuation.is_blocked() {
        return "team continuation blocked";
    }
    if continuation.status == "continue-with-remaining" {
        return "team continuation recorded";
    }
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "team dispatch ready",
        resource::ResourceLaneAdmission::SequentialFallback => "team dispatch sequential fallback",
        resource::ResourceLaneAdmission::Blocked => "team dispatch blocked",
    }
}

fn admission_event_type(
    admission: resource::ResourceLaneAdmission,
    blocked_by_policy: bool,
    blocked_by_ownership: bool,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "team.admission.blocked";
    }
    if blocked_by_ownership {
        return "team.admission.ownership_blocked";
    }
    if blocked_by_policy {
        return "team.admission.policy_blocked";
    }
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "team.admission.admitted",
        resource::ResourceLaneAdmission::SequentialFallback => "team.admission.fallback",
        resource::ResourceLaneAdmission::Blocked => "team.admission.blocked",
    }
}

fn admission_summary(
    admission: resource::ResourceLaneAdmission,
    blocked_by_policy: bool,
    blocked_by_ownership: bool,
) -> &'static str {
    if admission == resource::ResourceLaneAdmission::Blocked {
        return "team admission blocked";
    }
    if blocked_by_ownership {
        return "team admission ownership blocked";
    }
    if blocked_by_policy {
        return "team admission policy blocked";
    }
    match admission {
        resource::ResourceLaneAdmission::AllowParallel => "team admission admitted",
        resource::ResourceLaneAdmission::SequentialFallback => "team admission sequential fallback",
        resource::ResourceLaneAdmission::Blocked => "team admission blocked",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyGate {
    status: &'static str,
    checks: Vec<PolicyCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyCheck {
    target_type: &'static str,
    target: String,
    decision: policy::Decision,
    class: &'static str,
    approval_prompt: &'static str,
    reason: String,
}

impl PolicyGate {
    fn is_blocked(&self) -> bool {
        matches!(self.status, "approval-required" | "blocked")
    }

    fn blocked_label(&self) -> &'static str {
        if self.is_blocked() {
            "yes"
        } else {
            "no"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnershipGate {
    status: &'static str,
    checks: Vec<OwnershipCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnershipCheck {
    lane: u32,
    raw_path: String,
    normalized_path: String,
    status: &'static str,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedApprovalRequest {
    request_id: String,
    path: PathBuf,
}

impl OwnershipGate {
    fn is_blocked(&self) -> bool {
        matches!(self.status, "invalid" | "conflict")
    }

    fn blocked_label(&self) -> &'static str {
        if self.is_blocked() {
            "yes"
        } else {
            "no"
        }
    }
}

fn policy_write_paths(write_paths: &[String], owned_write_paths: &[(u32, String)]) -> Vec<String> {
    let mut paths = write_paths.to_vec();
    paths.extend(owned_write_paths.iter().map(|(_, path)| path.clone()));
    paths
}

fn policy_preflight(write_paths: &[String], commands: &[String]) -> Result<PolicyGate, AppError> {
    let mut checks = Vec::new();
    for path in write_paths {
        let decision = policy::classify_path(policy::PathMode::Write, path)?;
        checks.push(PolicyCheck {
            target_type: "write",
            target: path.clone(),
            decision: decision.decision,
            class: decision.command_class,
            approval_prompt: decision.approval_prompt,
            reason: decision.reason,
        });
    }
    for command in commands {
        let decision = policy::classify_command(command)?;
        checks.push(PolicyCheck {
            target_type: "command",
            target: ledger::redact_text(command),
            decision: decision.decision,
            class: decision.command_class,
            approval_prompt: decision.approval_prompt,
            reason: decision.reason,
        });
    }

    let status = if checks.is_empty() {
        "not-requested"
    } else if checks
        .iter()
        .any(|check| check.decision == policy::Decision::Deny)
    {
        "blocked"
    } else if checks
        .iter()
        .any(|check| check.decision == policy::Decision::Ask)
    {
        "approval-required"
    } else {
        "allowed"
    };

    Ok(PolicyGate { status, checks })
}

fn ownership_preflight(
    admitted_lanes: u32,
    owned_write_paths: &[(u32, String)],
) -> Result<OwnershipGate, AppError> {
    if owned_write_paths.is_empty() {
        return Ok(OwnershipGate {
            status: "not-requested",
            checks: Vec::new(),
        });
    }

    let mut owners: HashMap<String, u32> = HashMap::new();
    let mut checks = Vec::new();
    for (lane, raw_path) in owned_write_paths {
        let normalized_path = normalize_ownership_path(raw_path)?;
        let mut status = "assigned";
        let mut reason = "write path assigned to lane before dispatch".to_string();

        if *lane > admitted_lanes {
            status = "invalid";
            reason = format!(
                "lane {lane} exceeds admitted lanes {admitted_lanes}; reduce lanes or wait for resources"
            );
        } else if let Some(existing_lane) = owners.get(&normalized_path) {
            if *existing_lane != *lane {
                status = "conflict";
                reason = format!(
                    "path already owned by lane {existing_lane}; cross-lane writes are blocked"
                );
            }
        } else {
            owners.insert(normalized_path.clone(), *lane);
        }

        checks.push(OwnershipCheck {
            lane: *lane,
            raw_path: raw_path.clone(),
            normalized_path,
            status,
            reason,
        });
    }

    let status = if checks.iter().any(|check| check.status == "conflict") {
        "conflict"
    } else if checks.iter().any(|check| check.status == "invalid") {
        "invalid"
    } else {
        "allocated"
    };

    Ok(OwnershipGate { status, checks })
}

fn normalize_ownership_path(raw_path: &str) -> Result<String, AppError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(AppError::usage(
            "team admit의 owned write path는 비어 있을 수 없습니다.",
        ));
    }
    let path = Path::new(trimmed);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Ok(trimmed.to_string());
    }

    let project_root = canonical_project_root()?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    let normalized = normalize_existing_or_parent(&candidate)?;
    let relative = normalized
        .strip_prefix(&project_root)
        .unwrap_or(&normalized)
        .to_path_buf();
    Ok(path_key(&relative))
}

fn canonical_project_root() -> Result<PathBuf, AppError> {
    let root = paths::project_root();
    std::fs::create_dir_all(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 만들지 못했습니다: {} ({err})",
            root.display()
        ))
    })?;
    std::fs::canonicalize(&root).map_err(|err| {
        AppError::runtime(format!(
            "project root를 canonicalize하지 못했습니다: {} ({err})",
            root.display()
        ))
    })
}

fn normalize_existing_or_parent(path: &Path) -> Result<PathBuf, AppError> {
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|err| {
            AppError::runtime(format!(
                "path를 canonicalize하지 못했습니다: {} ({err})",
                path.display()
            ))
        });
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let canonical_parent = std::fs::canonicalize(parent).map_err(|err| {
        AppError::runtime(format!(
            "path parent를 canonicalize하지 못했습니다: {} ({err})",
            parent.display()
        ))
    })?;
    Ok(canonical_parent.join(path.file_name().unwrap_or_default()))
}

fn path_key(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            Component::RootDir => Some(String::new()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn record_approval_request(
    identity: &ledger::RuntimeIdentity,
    event: &ledger::LedgerEvent,
    admission_status: &str,
    policy_gate: &PolicyGate,
    ownership_gate: &OwnershipGate,
) -> Result<Option<RecordedApprovalRequest>, AppError> {
    if !policy_gate.is_blocked() && !ownership_gate.is_blocked() {
        return Ok(None);
    }

    let request_id = format!("team-{}", event.event_id);
    let mut items = Vec::new();
    items.extend(
        policy_gate
            .checks
            .iter()
            .filter(|check| check.decision != policy::Decision::Allow)
            .map(|check| {
                format!(
                    "policy {}: {} -> {} ({}, approval: {}, reason: {})",
                    check.target_type,
                    check.target,
                    decision_label(check.decision),
                    check.class,
                    check.approval_prompt,
                    check.reason
                )
            }),
    );
    items.extend(
        ownership_gate
            .checks
            .iter()
            .filter(|check| check.status != "assigned")
            .map(|check| {
                format!(
                    "ownership lane {}: {} -> {} (normalized: {}, reason: {})",
                    check.lane, check.raw_path, check.status, check.normalized_path, check.reason
                )
            }),
    );
    if items.is_empty() {
        items.push("team admission blocked; inspect ledger event for details".to_string());
    }

    let status = if policy_gate.status == "approval-required" && !ownership_gate.is_blocked() {
        "pending-approval"
    } else {
        "blocked"
    };
    let path = approval::write_request(&approval::ApprovalRequest {
        request_id: request_id.clone(),
        source: "team-admission".to_string(),
        status: status.to_string(),
        reason: admission_status.to_string(),
        event_id: event.event_id.clone(),
        session_id: identity.session_id.clone(),
        summary: event.summary.clone(),
        items,
    })?;

    Ok(Some(RecordedApprovalRequest { request_id, path }))
}

fn format_policy_checks(checks: &[PolicyCheck]) -> String {
    if checks.is_empty() {
        return "  - 없음".to_string();
    }

    checks
        .iter()
        .map(|check| {
            format!(
                "  - {}: {} -> {} ({}, approval: {}, reason: {})",
                check.target_type,
                check.target,
                decision_label(check.decision),
                check.class,
                check.approval_prompt,
                check.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_ownership_checks(checks: &[OwnershipCheck]) -> String {
    if checks.is_empty() {
        return "  - 없음".to_string();
    }

    checks
        .iter()
        .map(|check| {
            format!(
                "  - lane {}: {} -> {} (normalized: {}, reason: {})",
                check.lane, check.raw_path, check.status, check.normalized_path, check.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn decision_label(decision: policy::Decision) -> &'static str {
    match decision {
        policy::Decision::Allow => "allow",
        policy::Decision::Ask => "ask",
        policy::Decision::Deny => "deny",
    }
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values.join(", ")
    }
}

fn display_redacted_list(values: &[String]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values
            .iter()
            .map(|value| ledger::redact_text(value))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn display_owned_write_paths(values: &[(u32, String)]) -> String {
    if values.is_empty() {
        "없음".to_string()
    } else {
        values
            .iter()
            .map(|(lane, path)| format!("lane {lane}:{path}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn display_optional_lane(value: Option<u32>) -> String {
    value
        .map(|lane| lane.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

fn display_optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "없음".to_string())
}

fn display_optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

fn display_optional_u32(value: Option<u32>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "없음".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn status_falls_back_to_sequential_when_resource_sample_is_missing() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-status-no-sample-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        let report = status_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("team status"));
        assert!(report.contains("resource sample source: no-sample"));
        assert!(report.contains("resource pressure: unknown"));
        assert!(report.contains("admission: sequential-fallback"));
        assert!(report.contains("admitted lanes: 1"));
        assert!(report.contains("boundary: read-only"));
    }

    #[test]
    fn status_blocks_team_lanes_on_critical_resource_sample() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-status-critical-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        observability::record_resource_sample(&observability::ResourceSampleMetric {
            resource_sample_id: "resource-sample-team-critical".to_string(),
            session_id: "session-team-critical".to_string(),
            backend_id: "llama.cpp".to_string(),
            pid: 4242,
            process_cpu_percent: Some(98.0),
            average_rss_bytes: Some(14 * 1024 * 1024 * 1024),
            peak_rss_bytes: Some(14 * 1024 * 1024 * 1024),
            disk_bytes: Some(2048),
            sample_count: 1,
            pressure_status: "critical".to_string(),
            recorded_at_ms: 1234,
        })
        .unwrap();

        let report = status_report().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("resource sample source: latest-resource-sample"));
        assert!(report.contains("resource pressure: critical"));
        assert!(report.contains("resource cpu percent: 98.0"));
        assert!(report.contains("admission: blocked"));
        assert!(report.contains("dispatch blocked: yes"));
        assert!(report.contains("admitted lanes: 0"));
        assert!(report.contains("fallback: wait"));
    }

    #[test]
    fn admission_allows_parallel_and_records_ledger_event() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-admission-normal-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample("resource-sample-team-normal", "normal", Some(17.0));

        let report = admission_report(3, &[], &[], &["cargo test".to_string()]).unwrap();
        let store = observability::status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("team admission"));
        assert!(report.contains("status: admitted"));
        assert!(report.contains("requested parallel lanes: 3"));
        assert!(report.contains("admitted lanes: 3"));
        assert!(report.contains("admission: allow-parallel"));
        assert!(report.contains("dispatch blocked: no"));
        assert!(report.contains("policy checks: 1"));
        assert!(report.contains("policy status: allowed"));
        assert!(report.contains("command: cargo test -> allow"));
        assert!(report.contains("ownership claims: 0"));
        assert!(report.contains("ownership status: not-requested"));
        assert!(report.contains("approval request: not-required"));
        assert!(report.contains("ledger event: event-"));
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn admission_blocks_critical_pressure_and_records_ledger_event() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-admission-critical-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample(
            "resource-sample-team-admit-critical",
            "critical",
            Some(98.0),
        );

        let err = admission_report(4, &[], &[], &[]).unwrap_err();
        let store = observability::status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 3);
        assert!(err.message.contains("team admission 차단"));
        assert!(err.message.contains("status: blocked"));
        assert!(err.message.contains("resource pressure: critical"));
        assert!(err.message.contains("requested parallel lanes: 4"));
        assert!(err.message.contains("admitted lanes: 0"));
        assert!(err.message.contains("dispatch blocked: yes"));
        assert!(err.message.contains("approval request: not-required"));
        assert!(err.message.contains("ledger event: event-"));
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn admission_blocks_write_policy_preflight_and_records_ledger_event() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-admission-policy-write-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample("resource-sample-team-policy-write", "normal", Some(17.0));

        let err = admission_report(2, &["README.md".to_string()], &[], &[]).unwrap_err();
        let store = observability::status().unwrap();
        let approval_requests = approval::request_summaries(5).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 3);
        assert!(err.message.contains("status: policy-blocked"));
        assert!(err.message.contains("admission: allow-parallel"));
        assert!(err.message.contains("policy checks: 1"));
        assert!(err.message.contains("policy status: approval-required"));
        assert!(err.message.contains("write: README.md -> ask"));
        assert!(err.message.contains("dispatch blocked: yes"));
        assert!(err.message.contains("approval request: team-event-"));
        assert!(err.message.contains("approval request path:"));
        assert!(err.message.contains("ledger event: event-"));
        assert_eq!(approval_requests.len(), 1);
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn admission_reports_file_ownership_allocation() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-admission-ownership-test");
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(project_root.join("src/app.rs"), "").unwrap();
        fs::write(project_root.join("src/cli.rs"), "").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample("resource-sample-team-ownership", "normal", Some(17.0));

        let err = admission_report(
            2,
            &[],
            &[(1, "src/app.rs".to_string()), (2, "src/cli.rs".to_string())],
            &[],
        )
        .unwrap_err();
        let store = observability::status().unwrap();
        let approval_requests = approval::request_summaries(5).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 3);
        assert!(err.message.contains("status: policy-blocked"));
        assert!(err.message.contains("policy checks: 2"));
        assert!(err.message.contains("policy status: approval-required"));
        assert!(err.message.contains("ownership claims: 2"));
        assert!(err.message.contains("ownership status: allocated"));
        assert!(err.message.contains("ownership blocked: no"));
        assert!(err
            .message
            .contains("lane 1: src/app.rs -> assigned (normalized: src/app.rs"));
        assert!(err
            .message
            .contains("lane 2: src/cli.rs -> assigned (normalized: src/cli.rs"));
        assert!(err.message.contains("approval request: team-event-"));
        assert_eq!(approval_requests.len(), 1);
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn admission_blocks_cross_lane_file_ownership_conflict() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-admission-ownership-conflict-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        fs::write(project_root.join("README.md"), "").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample(
            "resource-sample-team-ownership-conflict",
            "normal",
            Some(17.0),
        );

        let err = admission_report(
            2,
            &[],
            &[(1, "README.md".to_string()), (2, "./README.md".to_string())],
            &[],
        )
        .unwrap_err();
        let store = observability::status().unwrap();
        let approval_requests = approval::request_summaries(5).unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 3);
        assert!(err.message.contains("status: ownership-blocked"));
        assert!(err.message.contains("policy status: approval-required"));
        assert!(err.message.contains("ownership status: conflict"));
        assert!(err.message.contains("ownership blocked: yes"));
        assert!(err
            .message
            .contains("lane 2: ./README.md -> conflict (normalized: README.md"));
        assert!(err.message.contains("approval request: team-event-"));
        assert_eq!(approval_requests.len(), 1);
        assert!(err.message.contains("ledger event: event-"));
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn dispatch_enforces_file_ownership_at_dispatch_time() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-dispatch-ownership-test");
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(project_root.join("src/team.rs"), "").unwrap();
        fs::write(project_root.join("src/cli.rs"), "").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample("resource-sample-team-dispatch-normal", "normal", Some(17.0));

        let report = dispatch_report(
            2,
            &[
                (1, "src/team.rs".to_string()),
                (2, "src/cli.rs".to_string()),
            ],
            None,
            None,
        )
        .unwrap();
        let store = observability::status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("team dispatch"));
        assert!(report.contains("status: admitted"));
        assert!(report.contains("lane admission: allow-parallel"));
        assert!(report.contains("dispatch blocked: no"));
        assert!(report.contains("ownership claims: 2"));
        assert!(report.contains("ownership status: allocated"));
        assert!(report.contains("ownership blocked: no"));
        assert!(report.contains("continuation status: not-requested"));
        assert!(report.contains("ledger event: event-"));
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn dispatch_blocks_cross_lane_file_ownership_conflict() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-dispatch-ownership-conflict-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        fs::write(project_root.join("README.md"), "").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample(
            "resource-sample-team-dispatch-conflict",
            "normal",
            Some(17.0),
        );

        let err = dispatch_report(
            2,
            &[(1, "README.md".to_string()), (2, "./README.md".to_string())],
            None,
            None,
        )
        .unwrap_err();
        let store = observability::status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 3);
        assert!(err.message.contains("team dispatch 차단"));
        assert!(err.message.contains("status: ownership-blocked"));
        assert!(err.message.contains("ownership status: conflict"));
        assert!(err.message.contains("ownership blocked: yes"));
        assert!(err
            .message
            .contains("lane 2: ./README.md -> conflict (normalized: README.md"));
        assert!(err.message.contains("ledger event: event-"));
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn dispatch_records_failed_worker_continuation_and_status_surfaces_it() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-dispatch-continuation-test");
        let project_root = root.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(project_root.join("src/team.rs"), "").unwrap();
        fs::write(project_root.join("src/cli.rs"), "").unwrap();
        fs::write(project_root.join("src/app.rs"), "").unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample(
            "resource-sample-team-dispatch-continuation",
            "normal",
            Some(17.0),
        );

        let dispatch = dispatch_report(
            3,
            &[
                (1, "src/team.rs".to_string()),
                (2, "src/cli.rs".to_string()),
                (3, "src/app.rs".to_string()),
            ],
            Some(2),
            Some("worker timed out"),
        )
        .unwrap();
        let status = status_report().unwrap();
        let store = observability::status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(dispatch.contains("status: continuation-ready"));
        assert!(dispatch.contains("failed lane: 2"));
        assert!(dispatch.contains("failure reason: worker timed out"));
        assert!(dispatch.contains("continuation status: continue-with-remaining"));
        assert!(dispatch.contains("continuation action: continue"));
        assert!(dispatch.contains("continuation remaining lanes: 2"));
        assert!(status.contains("latest team runtime event: team.continuation.recorded"));
        assert!(status.contains("latest team runtime summary: team continuation recorded"));
        assert!(status.contains("latest team runtime event id: event-"));
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn governor_clamps_context_and_records_ledger_event() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-governor-clamp-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample("resource-sample-team-governor-normal", "normal", Some(17.0));

        let report = governor_report(2, 6000, Some(4096), resource::ModelTier::Standard).unwrap();
        let store = observability::status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert!(report.contains("team governor"));
        assert!(report.contains("status: clamped"));
        assert!(report.contains("requested context tokens: 6000"));
        assert!(report.contains("context limit tokens: 4096"));
        assert!(report.contains("effective context tokens: 4096"));
        assert!(report.contains("context action: clamped"));
        assert!(report.contains("model route hint: escalate"));
        assert!(report.contains("dispatch blocked: no"));
        assert!(report.contains("ledger event: event-"));
        assert_eq!(store.ledger_events, 1);
    }

    #[test]
    fn governor_blocks_critical_pressure_and_records_ledger_event() {
        let _guard = crate::test_support::ENV_LOCK.lock().unwrap();
        let root = test_root("rpotato-team-governor-critical-test");
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        std::env::set_var("RPOTATO_PROJECT_ROOT", &project_root);
        std::env::set_var("RPOTATO_DATA_HOME", root.join("data"));

        record_resource_sample(
            "resource-sample-team-governor-critical",
            "critical",
            Some(98.0),
        );

        let err = governor_report(2, 1024, Some(4096), resource::ModelTier::Small).unwrap_err();
        let store = observability::status().unwrap();

        std::env::remove_var("RPOTATO_PROJECT_ROOT");
        std::env::remove_var("RPOTATO_DATA_HOME");
        let _ = fs::remove_dir_all(root);

        assert_eq!(err.code, 3);
        assert!(err.message.contains("team governor 차단"));
        assert!(err.message.contains("status: blocked"));
        assert!(err.message.contains("resource pressure: critical"));
        assert!(err.message.contains("context action: blocked"));
        assert!(err.message.contains("model route hint: defer"));
        assert!(err.message.contains("dispatch blocked: yes"));
        assert!(err.message.contains("ledger event: event-"));
        assert_eq!(store.ledger_events, 1);
    }

    fn record_resource_sample(id: &str, pressure_status: &str, cpu: Option<f64>) {
        observability::record_resource_sample(&observability::ResourceSampleMetric {
            resource_sample_id: id.to_string(),
            session_id: format!("session-{id}"),
            backend_id: "llama.cpp".to_string(),
            pid: 4242,
            process_cpu_percent: cpu,
            average_rss_bytes: Some(512 * 1024 * 1024),
            peak_rss_bytes: Some(512 * 1024 * 1024),
            disk_bytes: Some(2048),
            sample_count: 1,
            pressure_status: pressure_status.to_string(),
            recorded_at_ms: 1234,
        })
        .unwrap();
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }
}
