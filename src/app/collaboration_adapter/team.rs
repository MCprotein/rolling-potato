use crate::adapters::filesystem::layout as paths;
use crate::app::approval_adapter as approval;
use crate::app::collaboration_adapter::team_state;
use crate::app::observability_adapter as observability;
use crate::app::policy_adapter as policy;
use crate::app::workflow_adapter::ledger;
use crate::app::workflow_adapter::state;
use crate::foundation::error::AppError;
use crate::runtime_core::collaboration::team::{
    admission_event_type, admission_summary, continuation_decision, decision_label,
    dispatch_event_type, dispatch_status, dispatch_summary, evaluate_ownership_gate,
    evaluate_policy_gate, governor_event_type, governor_status, governor_summary,
    is_team_runtime_event, overall_status, policy_write_paths, pressure_from_status,
    OwnershipCheck, OwnershipClaim, OwnershipGate, PolicyCheck, PolicyGate,
};
use crate::runtime_core::inference::resource;
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
    let policy_gate = evaluate_policy_gate(classify_policy_inputs(&policy_write_paths, commands)?);
    let ownership_gate = evaluate_ownership_gate(
        decision.admitted_lanes,
        normalize_ownership_claims(owned_write_paths)?,
    );
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
    let ownership_gate = evaluate_ownership_gate(
        lane_decision.admitted_lanes,
        normalize_ownership_claims(owned_write_paths)?,
    );
    let continuation = continuation_decision(
        lane_decision.admitted_lanes,
        failed_lane,
        &ledger::redact_text(failure_reason.unwrap_or("not-provided")),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedApprovalRequest {
    request_id: String,
    path: PathBuf,
}

fn classify_policy_inputs(
    write_paths: &[String],
    commands: &[String],
) -> Result<Vec<PolicyCheck>, AppError> {
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

    Ok(checks)
}

fn normalize_ownership_claims(
    owned_write_paths: &[(u32, String)],
) -> Result<Vec<OwnershipClaim>, AppError> {
    let mut claims = Vec::new();
    for (lane, raw_path) in owned_write_paths {
        let normalized_path = normalize_ownership_path(raw_path)?;
        claims.push(OwnershipClaim {
            lane: *lane,
            raw_path: raw_path.clone(),
            normalized_path,
        });
    }
    Ok(claims)
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
#[path = "team/tests.rs"]
mod tests;
